//! The sign-in chain against a local mock of the Microsoft, Xbox Live and Minecraft endpoints
//! (recorded response shapes, mock tokens only), and the keychain abstraction in memory.
use super::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

const UUID: &str = "069a79f444e94726a5befca90e38aaf5";

/// What the mock answers and what it was asked.
#[derive(Default)]
struct Mock {
    /// Polls left before the token endpoint stops saying authorization_pending.
    pending: usize,
    dropped_polls: usize,
    unavailable_polls: usize,
    declined: bool,
    minecraft_status: u16,
    invalid_registration: bool,
    xerr: Option<u64>,
    owns_game: bool,
    no_profile: bool,
    /// Every request: method, path and body.
    log: Vec<(String, String, String)>,
}

fn reply(status: u16, body: Value) -> (u16, String) {
    (status, body.to_string())
}

fn route(mock: &mut Mock, method: &str, path: &str, auth: &str, body: &str) -> (u16, String) {
    let form = |key: &str| {
        body.split('&')
            .filter_map(|p| p.split_once('='))
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.replace('+', " ").replace("%3A", ":").replace("%20", " "))
    };
    match (method, path) {
        ("POST", "/ms/devicecode") => {
            assert_eq!(form("client_id").as_deref(), Some("test-client"));
            assert_eq!(form("scope").as_deref(), Some(SCOPE));
            reply(
                200,
                json!({"user_code": "ABCD-EFGH", "device_code": "dev-1", "verification_uri": "https://www.microsoft.com/link", "expires_in": 900, "interval": 1}),
            )
        }
        ("POST", "/ms/token") => match form("grant_type").as_deref() {
            Some("urn:ietf:params:oauth:grant-type:device_code") if mock.dropped_polls > 0 => {
                mock.dropped_polls -= 1;
                reply(0, Value::Null)
            }
            Some("urn:ietf:params:oauth:grant-type:device_code") if mock.unavailable_polls > 0 => {
                mock.unavailable_polls -= 1;
                reply(503, json!({"error": "temporarily_unavailable"}))
            }
            Some("urn:ietf:params:oauth:grant-type:device_code") if mock.declined => {
                reply(400, json!({"error": "authorization_declined"}))
            }
            Some("urn:ietf:params:oauth:grant-type:device_code") if mock.pending > 0 => {
                mock.pending -= 1;
                reply(400, json!({"error": "authorization_pending"}))
            }
            Some("urn:ietf:params:oauth:grant-type:device_code") => {
                assert_eq!(form("device_code").as_deref(), Some("dev-1"));
                reply(
                    200,
                    json!({"access_token": "ms-access-1", "refresh_token": "ms-refresh-1", "expires_in": 3600}),
                )
            }
            Some("refresh_token") if form("refresh_token").as_deref() == Some("ms-refresh-1") => {
                reply(
                    200,
                    json!({"access_token": "ms-access-2", "refresh_token": "ms-refresh-2"}),
                )
            }
            _ => reply(400, json!({"error": "invalid_grant"})),
        },
        ("POST", "/xbl") => {
            let v: Value = serde_json::from_str(body).unwrap();
            assert!(
                v["Properties"]["RpsTicket"]
                    .as_str()
                    .unwrap()
                    .starts_with("d=ms-access-")
            );
            assert_eq!(v["RelyingParty"], "http://auth.xboxlive.com");
            reply(
                200,
                json!({"Token": "xbl-token", "DisplayClaims": {"xui": [{"uhs": "uhs-1"}]}}),
            )
        }
        ("POST", "/xsts") => {
            let v: Value = serde_json::from_str(body).unwrap();
            assert_eq!(v["Properties"]["UserTokens"][0], "xbl-token");
            assert_eq!(v["RelyingParty"], "rp://api.minecraftservices.com/");
            match mock.xerr {
                Some(code) => reply(401, json!({"Identity": "0", "XErr": code, "Message": ""})),
                None => reply(
                    200,
                    json!({"Token": "xsts-token", "DisplayClaims": {"xui": [{"uhs": "uhs-1", "xid": "2535400000000000"}]}}),
                ),
            }
        }
        ("POST", "/mc/authentication/login_with_xbox") => {
            if mock.minecraft_status != 0 {
                return reply(
                    mock.minecraft_status,
                    if mock.invalid_registration {
                        json!({"errorMessage": "Invalid app registration"})
                    } else {
                        json!({"error": "ForbiddenOperationException"})
                    },
                );
            }
            let v: Value = serde_json::from_str(body).unwrap();
            assert_eq!(v["identityToken"], "XBL3.0 x=uhs-1;xsts-token");
            reply(
                200,
                json!({"username": "mock", "access_token": "mc-token", "token_type": "Bearer", "expires_in": 86400}),
            )
        }
        ("GET", "/mc/entitlements/mcstore") => {
            assert_eq!(auth, "Bearer mc-token");
            let items = if mock.owns_game {
                json!([{"name": "product_minecraft", "signature": "x"}, {"name": "game_minecraft", "signature": "x"}])
            } else {
                json!([])
            };
            reply(200, json!({"items": items, "signature": "x", "keyId": "1"}))
        }
        ("GET", "/mc/minecraft/profile") if mock.no_profile => reply(
            404,
            json!({"path": "/minecraft/profile", "errorType": "NOT_FOUND", "error": "NOT_FOUND"}),
        ),
        ("GET", "/mc/minecraft/profile") => reply(
            200,
            json!({
                "id": UUID, "name": "Notch",
                "skins": [{"id": "s1", "state": "ACTIVE", "url": "http://textures.minecraft.net/texture/abc", "variant": "SLIM"}],
                "capes": [{"id": "c1", "state": "INACTIVE", "url": "http://textures.minecraft.net/texture/cape1", "alias": "Migrator"},
                          {"id": "c2", "state": "ACTIVE", "url": "http://textures.minecraft.net/texture/cape2", "alias": "Vanilla"}],
            }),
        ),
        _ => reply(404, json!({"error": "NotFound"})),
    }
}

/// A one-thread HTTP/1.1 server: one request per connection.
fn serve(mock: Arc<Mutex<Mock>>) -> Endpoints {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (method, path) = (
                parts.next().unwrap_or("").to_owned(),
                parts.next().unwrap_or("").to_owned(),
            );
            let (mut length, mut auth) = (0, String::new());
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                let header = header.trim_end();
                if header.is_empty() {
                    break;
                }
                let (k, v) = header.split_once(':').unwrap();
                match k.to_ascii_lowercase().as_str() {
                    "content-length" => length = v.trim().parse().unwrap(),
                    "authorization" => auth = v.trim().to_owned(),
                    _ => {}
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let body = String::from_utf8(body).unwrap();
            let (status, text) = {
                let mut m = mock.lock().unwrap();
                m.log.push((method.clone(), path.clone(), body.clone()));
                route(&mut m, &method, &path, &auth, &body)
            };
            if status == 0 {
                continue;
            } // Drop the connection, simulating Android's network abort.
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            );
        }
    });
    Endpoints {
        microsoft: format!("{base}/ms"),
        xbox_user: format!("{base}/xbl"),
        xsts: format!("{base}/xsts"),
        minecraft: format!("{base}/mc"),
    }
}

fn mock(f: impl FnOnce(&mut Mock)) -> (Arc<Mutex<Mock>>, Endpoints) {
    let mut m = Mock {
        owns_game: true,
        ..Default::default()
    };
    f(&mut m);
    let m = Arc::new(Mutex::new(m));
    let ep = serve(m.clone());
    (m, ep)
}

#[test]
fn the_whole_chain() {
    let (m, ep) = mock(|m| m.pending = 2);
    let code = device_code(&ep, "test-client").unwrap();
    assert_eq!(
        (code.user_code.as_str(), code.verification_uri.as_str()),
        ("ABCD-EFGH", "https://www.microsoft.com/link")
    );
    let ms = wait_for_sign_in(&ep, "test-client", &code, &|| false).unwrap();
    let (account, secrets) = finish_sign_in(&ep, ms).unwrap();
    assert_eq!(
        account,
        Account {
            uuid: UUID.into(),
            name: "Notch".into(),
            skin: Some((
                "http://textures.minecraft.net/texture/abc".into(),
                "slim".into()
            )),
            cape: Some("http://textures.minecraft.net/texture/cape2".into()),
        }
    );
    assert_eq!(secrets.minecraft_token, "mc-token");
    assert_eq!(secrets.xuid.as_deref(), Some("2535400000000000"));
    assert!(secrets.minecraft_expires_at > now() + 80000);
    // Two pending polls, then the token; then Xbox, XSTS, Minecraft, entitlements, profile.
    let paths: Vec<String> = m
        .lock()
        .unwrap()
        .log
        .iter()
        .map(|(_, p, _)| p.clone())
        .collect();
    assert_eq!(
        paths,
        [
            "/ms/devicecode",
            "/ms/token",
            "/ms/token",
            "/ms/token",
            "/xbl",
            "/xsts",
            "/mc/authentication/login_with_xbox",
            "/mc/entitlements/mcstore",
            "/mc/minecraft/profile"
        ]
    );
    assert!(!format!("{secrets:?}").contains("mc-token"));
}

#[test]
fn refresh_only_when_the_token_runs_out() {
    let (m, ep) = mock(|_| {});
    let account = Account {
        uuid: UUID.into(),
        name: "Notch".into(),
        skin: None,
        cape: None,
    };
    let good = Secrets {
        refresh_token: "ms-refresh-1".into(),
        minecraft_token: "old".into(),
        minecraft_expires_at: now() + 7200,
        xuid: None,
    };
    let (_, kept) = fresh(&ep, "test-client", &account, good).unwrap();
    assert_eq!(kept.minecraft_token, "old");
    assert!(m.lock().unwrap().log.is_empty());
    let stale = Secrets {
        minecraft_expires_at: now() + 60,
        ..kept.clone()
    };
    let (renewed_account, renewed) = fresh(&ep, "test-client", &account, stale).unwrap();
    assert_eq!(
        (
            renewed.refresh_token.as_str(),
            renewed.minecraft_token.as_str()
        ),
        ("ms-refresh-2", "mc-token")
    );
    assert!(renewed_account.skin.is_some());
    let revoked = Secrets {
        refresh_token: "revoked".into(),
        minecraft_expires_at: 0,
        ..kept
    };
    assert_eq!(
        fresh(&ep, "test-client", &account, revoked).unwrap_err(),
        SIGN_IN_AGAIN
    );
}

#[test]
fn ownership_is_asked_again_with_the_saved_token() {
    let secrets = |token: &str| Secrets {
        refresh_token: "ms-refresh-1".into(),
        minecraft_token: token.into(),
        minecraft_expires_at: now() + 7200,
        xuid: None,
    };
    let (m, ep) = mock(|_| {});
    assert_eq!(owns_minecraft(&ep, &secrets("mc-token")), Ok(true));
    assert_eq!(m.lock().unwrap().log.len(), 1);
    let (_, ep) = mock(|m| m.owns_game = false);
    assert_eq!(owns_minecraft(&ep, &secrets("mc-token")), Ok(false));
}

#[test]
fn refusals_have_messages() {
    let tokens = || MsTokens {
        access: "ms-access-1".into(),
        refresh: "r".into(),
    };
    let (_, ep) = mock(|m| m.xerr = Some(2148916233));
    assert!(
        finish_sign_in(&ep, tokens())
            .unwrap_err()
            .contains("no Xbox profile")
    );
    let (_, ep) = mock(|m| m.xerr = Some(2148916238));
    assert!(
        finish_sign_in(&ep, tokens())
            .unwrap_err()
            .contains("child account")
    );
    let (_, ep) = mock(|m| m.owns_game = false);
    assert_eq!(
        finish_sign_in(&ep, tokens()).unwrap_err(),
        "This account doesn't own Minecraft: Java Edition."
    );
    let (_, ep) = mock(|m| m.no_profile = true);
    assert!(
        finish_sign_in(&ep, tokens())
            .unwrap_err()
            .contains("no Minecraft profile")
    );
    assert_eq!(
        oauth_error(&json!({"error": "authorization_declined"})),
        "Sign-in was declined in the browser."
    );
    // Nothing listening: a message, not a URL.
    let dead = Endpoints {
        microsoft: "http://127.0.0.1:9/ms".into(),
        ..Endpoints::live()
    };
    assert_eq!(
        device_code(&dead, "x").unwrap_err(),
        "No se pudo contactar con Microsoft. Intenta de nuevo cuando vuelva la conexión."
    );
}

#[test]
fn cancelling_stops_the_wait() {
    let (_, ep) = mock(|m| m.pending = 100);
    let code = device_code(&ep, "test-client").unwrap();
    assert_eq!(
        wait_for_sign_in(&ep, "test-client", &code, &|| true).unwrap_err(),
        "Sign-in cancelled."
    );
}

#[test]
fn public_client_id_is_a_guid() {
    assert!(valid_client_id("12345678-abcd-1234-abcd-1234567890ab"));
    assert!(!valid_client_id(""));
    assert!(!valid_client_id("borrowed-token"));
}
#[test]
fn errors_never_echo_remote_secrets() {
    assert!(!oauth_error(&json!({"error": "secret-token"})).contains("secret-token"));
}

#[test]
fn account_textures_stay_on_the_official_https_cdn() {
    assert_eq!(
        official_texture_url("http://textures.minecraft.net/texture/abc"),
        Some("https://textures.minecraft.net/texture/abc".into())
    );
    assert!(official_texture_url("https://textures.minecraft.net.evil/texture/abc").is_none());
    assert!(
        official_texture_url("https://textures.minecraft.net/texture/abc?token=secret").is_none()
    );
    assert!(official_texture_url("https://textures.minecraft.net/texture/../private").is_none());
}

#[test]
fn polling_keeps_the_same_grant_after_connection_abort_and_503() {
    let (m, ep) = mock(|m| {
        m.dropped_polls = 1;
        m.unavailable_polls = 1;
    });
    let code = device_code(&ep, "test-client").unwrap();
    let tokens = wait_for_sign_in(&ep, "test-client", &code, &|| false).unwrap();
    assert_eq!(tokens.access, "ms-access-1");
    let log = &m.lock().unwrap().log;
    assert_eq!(
        log.iter().filter(|(_, p, _)| p == "/ms/devicecode").count(),
        1
    );
    assert!(log.iter().filter(|(_, p, _)| p == "/ms/token").count() >= 3);
    assert!(
        log.iter()
            .filter(|(_, p, _)| p == "/ms/token")
            .all(|(_, _, body)| body.contains("device_code=dev-1"))
    );
}

#[test]
fn returning_from_browser_resumes_without_a_new_grant() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let (m, ep) = mock(|_| {});
    let code = device_code(&ep, "test-client").unwrap();
    let active = Arc::new(AtomicBool::new(false));
    let flag = active.clone();
    let worker = std::thread::spawn(move || {
        wait_for_sign_in_while_active(&ep, "test-client", &code, &|| false, &|| {
            flag.load(Ordering::Relaxed)
        })
    });
    std::thread::sleep(Duration::from_millis(1250));
    assert_eq!(
        m.lock().unwrap().log.len(),
        1,
        "no polling while backgrounded"
    );
    active.store(true, Ordering::Relaxed);
    assert_eq!(worker.join().unwrap().unwrap().access, "ms-access-1");
    assert_eq!(m.lock().unwrap().log.len(), 2);
}

#[test]
fn backgrounded_grants_still_expire_and_cancel() {
    let (_, ep) = mock(|_| {});
    let mut code = device_code(&ep, "test-client").unwrap();
    assert!(
        wait_for_sign_in_while_active(&ep, "test-client", &code, &|| true, &|| false)
            .unwrap_err()
            .contains("cancelled")
    );
    code.expires_at = now().saturating_sub(1);
    assert!(
        wait_for_sign_in_while_active(&ep, "test-client", &code, &|| false, &|| false)
            .unwrap_err()
            .contains("expired")
    );
}

#[test]
fn declined_authorization_is_not_retried() {
    let (m, ep) = mock(|m| m.declined = true);
    let code = device_code(&ep, "test-client").unwrap();
    assert_eq!(
        wait_for_sign_in(&ep, "test-client", &code, &|| false).unwrap_err(),
        "Sign-in was declined in the browser."
    );
    assert_eq!(m.lock().unwrap().log.len(), 2);
}

#[test]
fn minecraft_denials_are_distinct_from_network_failure() {
    let tokens = || MsTokens {
        access: "ms-access-1".into(),
        refresh: "mock-refresh".into(),
    };
    for (status, registration) in [(403, true), (403, false), (401, false)] {
        let (_, ep) = mock(|m| {
            m.minecraft_status = status;
            m.invalid_registration = registration;
        });
        let error = finish_sign_in(&ep, tokens()).unwrap_err();
        assert!(error.contains(&format!("HTTP {status}")));
        assert_eq!(error.contains("Mojang"), registration);
        assert!(!error.contains("contactar"));
    }
}

#[test]
fn native_cached_sessions_recheck_entitlement_and_profile_before_play() {
    let account = Account {
        uuid: UUID.into(),
        name: "Notch".into(),
        skin: None,
        cape: None,
    };
    let secrets = || Secrets {
        refresh_token: "ms-refresh-1".into(),
        minecraft_token: "mc-token".into(),
        minecraft_expires_at: now() + 7200,
        xuid: None,
    };
    let (m, ep) = mock(|_| {});
    let (checked, tokens) = verified_fresh(&ep, "test-client", &account, secrets()).unwrap();
    assert!(tokens.verified_session(&checked).unwrap().java_verified());
    let paths: Vec<_> = m
        .lock()
        .unwrap()
        .log
        .iter()
        .map(|(_, p, _)| p.clone())
        .collect();
    assert_eq!(paths, ["/mc/entitlements/mcstore", "/mc/minecraft/profile"]);
    let (_, ep) = mock(|m| m.owns_game = false);
    assert!(
        verified_fresh(&ep, "test-client", &account, secrets())
            .unwrap_err()
            .contains("doesn't own")
    );
    let (_, ep) = mock(|m| m.no_profile = true);
    assert!(verified_fresh(&ep, "test-client", &account, secrets()).is_err());
    let (_, ep) = mock(|_| {});
    let mut other = account;
    other.uuid = "00000000000000000000000000000001".into();
    assert!(
        verified_fresh(&ep, "test-client", &other, secrets())
            .unwrap_err()
            .contains("profile changed")
    );
}
