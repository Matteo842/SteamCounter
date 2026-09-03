use std::{
    io::{Read, Write},
    net::TcpListener,
    thread::{self, JoinHandle},
    time::Instant,
};

use super::*;

// Risposte HTTP locali: i test non dipendono da Internet o da conteggi variabili.
pub(super) fn fixture(
    responses: Vec<(&'static str, u16, &'static str)>,
) -> (SteamClient, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let worker = thread::spawn(move || {
        for (expected_path, status, body) in responses {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            Instant::now() < deadline,
                            "Richiesta HTTP attesa: {expected_path}"
                        );
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            // Windows può ereditare la modalità non bloccante dal listener.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0, "Richiesta HTTP incompleta");
                request.extend_from_slice(&buffer[..count]);
            }
            let request = String::from_utf8(request).unwrap();
            assert!(
                request.starts_with(&format!("GET {expected_path} HTTP/1.1\r\n")),
                "{request}"
            );
            write!(stream,
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ).unwrap();
        }
    });
    let client = SteamClient {
        http: Client::builder().no_proxy().build().unwrap(),
        players_url: format!("{base}/players"),
        search_url: format!("{base}/search"),
        details_url: format!("{base}/details"),
        timeout: Duration::from_secs(2),
    };
    (client, worker)
}

fn game(appid: u32, name: &str) -> Game {
    Game {
        appid: NonZeroU32::new(appid).unwrap(),
        name: name.to_owned(),
    }
}

#[test]
fn name_to_counter_uses_exact_match_and_encodes_search_terms() {
    let (client, server) = fixture(vec![
        (
            "/search?term=Tom+%26+Jerry&l=english&cc=IT",
            200,
            r#"{
            "total": 4,
            "items": [
                {"type":"app","id":2,"name":"Tom & Jerry DLC"},
                {"type":"app","id":1,"name":"TOM & JERRY"},
                {"type":"app","id":1,"name":"TOM & JERRY"},
                {"type":"bundle","id":3,"name":"Tom & Jerry"}
            ]
        }"#,
        ),
        (
            "/players?appid=1",
            200,
            r#"{"response":{"result":1,"player_count":12345}}"#,
        ),
    ]);
    let games = client.search("Tom & Jerry").unwrap();
    assert_eq!(games.len(), 2);
    let NameMatch::Found(game) = match_name("Tom & Jerry", games) else {
        panic!("Nome esatto atteso")
    };
    let before = Utc::now();
    let snapshot = client.snapshot(game.appid, Some(game.name)).unwrap();
    assert_eq!(snapshot.appid.get(), 1);
    assert_eq!(snapshot.player_count, 12345);
    assert!(snapshot.checked_at >= before && snapshot.checked_at <= Utc::now());
    let json = serde_json::to_value(snapshot).unwrap();
    assert_eq!(json["name"], "TOM & JERRY");
    assert_eq!(json["appid"], 1);
    assert_eq!(json["player_count"], 12345);
    assert!(json["checked_at"].as_str().unwrap().ends_with('Z'));
    server.join().unwrap();
}

#[test]
fn appid_lookup_adds_store_name() {
    let (client, server) = fixture(vec![
        (
            "/players?appid=730",
            200,
            r#"{"response":{"result":1,"player_count":123}}"#,
        ),
        (
            "/details?appids=730&filters=basic&l=english&cc=IT",
            200,
            r#"{"730":{"success":true,"data":{"name":"Counter-Strike 2"}}}"#,
        ),
    ]);
    let snapshot = client
        .snapshot(NonZeroU32::new(730).unwrap(), None)
        .unwrap();
    assert_eq!(snapshot.name.as_deref(), Some("Counter-Strike 2"));
    assert_eq!(snapshot.player_count, 123);
    server.join().unwrap();
}

#[test]
fn zero_players_is_valid_even_when_store_is_down() {
    let (client, server) = fixture(vec![
        (
            "/players?appid=730",
            200,
            r#"{"response":{"result":1,"player_count":0}}"#,
        ),
        (
            "/details?appids=730&filters=basic&l=english&cc=IT",
            503,
            "unavailable",
        ),
    ]);
    let snapshot = client
        .snapshot(NonZeroU32::new(730).unwrap(), None)
        .unwrap();
    assert_eq!(snapshot.player_count, 0);
    assert!(snapshot.name.is_none());
    server.join().unwrap();
}

#[test]
fn api_failures_never_become_zero_players() {
    for (status, body, message) in [
        (
            200,
            r#"{"response":{"result":42,"player_count":0}}"#,
            "code 42",
        ),
        (
            200,
            r#"{"response":{"result":1}}"#,
            "does not contain a player count",
        ),
        (
            200,
            r#"{"response":{"result":1,"player_count":null}}"#,
            "does not contain a player count",
        ),
        (
            200,
            "<html>unavailable</html>",
            "invalid or incompatible JSON",
        ),
        (429, "rate limit", "too many requests"),
        (500, "server error", "HTTP error"),
    ] {
        let (client, server) = fixture(vec![("/players?appid=730", status, body)]);
        let error = client
            .snapshot(NonZeroU32::new(730).unwrap(), Some("CS2".to_owned()))
            .unwrap_err();
        assert!(error.to_string().contains(message), "{error:#}");
        server.join().unwrap();
    }
}

#[test]
fn malformed_search_is_an_error_but_empty_search_is_valid() {
    for (body, valid) in [(r#"{"total":0,"items":[]}"#, true), (r#"{}"#, false)] {
        let (client, server) = fixture(vec![("/search?term=missing&l=english&cc=IT", 200, body)]);
        let result = client.search("missing");
        if valid {
            assert!(result.unwrap().is_empty());
        } else {
            assert!(result.is_err());
        }
        server.join().unwrap();
    }
}

#[test]
fn ambiguous_names_are_not_silently_selected() {
    let games = vec![game(1, "Portal"), game(2, "Portal 2")];
    assert!(matches!(
        match_name("port", games.clone()),
        NameMatch::Ambiguous(_)
    ));
    assert_eq!(
        match_name("  PORTAL  ", games),
        NameMatch::Found(game(1, "Portal"))
    );
    assert!(matches!(
        match_name("Portal", vec![game(1, "Portal"), game(2, "Portal")]),
        NameMatch::Ambiguous(_)
    ));
    assert_eq!(match_name("missing", vec![]), NameMatch::NotFound);
    assert_eq!(
        match_name("portal", vec![game(2, "Portal 2")]),
        NameMatch::Found(game(2, "Portal 2"))
    );
}

#[test]
fn input_validation_distinguishes_appids_from_names_and_user_steamids() {
    assert_eq!(
        GameQuery::parse(" 730 ").unwrap(),
        GameQuery::AppId(NonZeroU32::new(730).unwrap())
    );
    assert_eq!(
        GameQuery::parse("ELDEN RING").unwrap(),
        GameQuery::Name("ELDEN RING".to_owned())
    );
    for invalid in ["", "   ", "0", "4294967296", "76561198000000000"] {
        assert!(GameQuery::parse(invalid).is_err(), "{invalid}");
    }
}
