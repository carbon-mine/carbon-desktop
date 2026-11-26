use crate::AccountError;
use reqwest::{
    blocking::{Client, RequestBuilder},
    header::{ACCEPT, CONTENT_TYPE, RETRY_AFTER},
    redirect::Policy,
};
use serde_json::json;
use std::{io::Read, time::Duration};
use url::form_urlencoded;
use uuid::Uuid;
use zeroize::Zeroizing;

pub(super) const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);
const RESPONSE_LIMIT: u64 = 256 * 1024;
const SCOPE: &str = "XboxLive.signin offline_access";
const DEVICE: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode";
const TOKEN: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
const XBOX: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MINECRAFT: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const ENTITLEMENTS: &str = "https://api.minecraftservices.com/entitlements/mcstore";
const PROFILE: &str = "https://api.minecraftservices.com/minecraft/profile";

pub(super) enum Request<'a> {
    Device(Uuid),
    Poll(Uuid, &'a str),
    Refresh(Uuid, &'a str),
    XboxUser(&'a str),
    XboxTitle(&'a str),
    Minecraft(&'a str, &'a str),
    Entitlements(&'a str),
    Profile(&'a str),
}

pub(super) struct Response {
    pub status: u16,
    pub body: Zeroizing<Vec<u8>>,
    pub retry_after: Option<Duration>,
}

pub(super) trait Transport {
    fn send(&self, request: Request<'_>, timeout: Duration) -> Result<Response, AccountError>;
}

pub(super) struct Http(Client);

impl Http {
    pub(super) fn new() -> Result<Self, AccountError> {
        let client = Client::builder()
            .user_agent("Carbon Desktop")
            .https_only(true)
            .redirect(Policy::none())
            .referer(false)
            .no_proxy()
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(5))
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|_| AccountError::TemporarilyUnavailable)?;
        Ok(Self(client))
    }

    fn form(&self, endpoint: &str, fields: &[(&str, &str)]) -> RequestBuilder {
        let mut form = form_urlencoded::Serializer::new(String::new());
        form.extend_pairs(fields.iter().copied());
        self.0
            .post(endpoint)
            .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(form.finish())
    }

    fn json(&self, endpoint: &str, payload: String) -> RequestBuilder {
        self.0
            .post(endpoint)
            .header(CONTENT_TYPE, "application/json")
            .body(payload)
    }

    fn request(&self, request: Request<'_>) -> RequestBuilder {
        match request {
            Request::Device(id) => {
                self.form(DEVICE, &[("client_id", &id.to_string()), ("scope", SCOPE)])
            }
            Request::Poll(id, device_code) => self.form(
                TOKEN,
                &[
                    ("client_id", &id.to_string()),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                    ("device_code", device_code),
                ],
            ),
            Request::Refresh(id, refresh_token) => self.form(
                TOKEN,
                &[
                    ("client_id", &id.to_string()),
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh_token),
                    ("scope", SCOPE),
                ],
            ),
            Request::XboxUser(token) => self
                .json(
                    XBOX,
                    json!({
                        "Properties": {
                            "AuthMethod": "RPS",
                            "SiteName": "user.auth.xboxlive.com",
                            "RpsTicket": Zeroizing::new(format!("d={token}")).as_str(),
                        },
                        "RelyingParty": "http://auth.xboxlive.com",
                        "TokenType": "JWT",
                    })
                    .to_string(),
                )
                .header("x-xbl-contract-version", "1"),
            Request::XboxTitle(token) => self
                .json(
                    XSTS,
                    json!({
                        "Properties": { "SandboxId": "RETAIL", "UserTokens": [token] },
                        "RelyingParty": "rp://api.minecraftservices.com/",
                        "TokenType": "JWT",
                    })
                    .to_string(),
                )
                .header("x-xbl-contract-version", "1"),
            Request::Minecraft(hash, token) => self.json(
                MINECRAFT,
                json!({
                    "identityToken": Zeroizing::new(format!("XBL3.0 x={hash};{token}")).as_str(),
                })
                .to_string(),
            ),
            Request::Entitlements(token) => self.0.get(ENTITLEMENTS).bearer_auth(token),
            Request::Profile(token) => self.0.get(PROFILE).bearer_auth(token),
        }
    }
}

impl Transport for Http {
    fn send(&self, request: Request<'_>, timeout: Duration) -> Result<Response, AccountError> {
        let response = self
            .request(request)
            .header(ACCEPT, "application/json")
            .timeout(timeout)
            .send()
            .map_err(|_| AccountError::TemporarilyUnavailable)?;
        let status = response.status().as_u16();
        // Delay until the challenge expires when advertised syntax can't be interpreted.
        let retry_after = response.headers().get(RETRY_AFTER).map(|header| {
            header
                .to_str()
                .ok()
                .and_then(|value| value.parse().ok())
                .map_or(Duration::MAX, Duration::from_secs)
        });
        let mut body = Zeroizing::new(Vec::new());
        if status == 429 || status >= 500 {
            return Ok(Response {
                status,
                body,
                retry_after,
            });
        }
        if response
            .content_length()
            .is_some_and(|size| size > RESPONSE_LIMIT)
        {
            return Err(AccountError::InvalidResponse);
        }
        response
            .take(RESPONSE_LIMIT + 1)
            .read_to_end(&mut body)
            .map_err(|_| AccountError::TemporarilyUnavailable)?;
        if body.len() as u64 > RESPONSE_LIMIT {
            return Err(AccountError::InvalidResponse);
        }
        Ok(Response {
            status,
            body,
            retry_after,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_are_bound_to_the_expected_origin_and_credentials() {
        let transport = Http::new().unwrap();
        let id = Uuid::from_u128(1);
        let device = transport.request(Request::Device(id)).build().unwrap();
        assert_eq!(device.url().as_str(), DEVICE);
        let form = |request: &reqwest::blocking::Request| {
            form_urlencoded::parse(request.body().unwrap().as_bytes().unwrap())
                .into_owned()
                .collect::<std::collections::BTreeMap<_, _>>()
        };
        let device_fields = form(&device);
        assert_eq!(device_fields["scope"], SCOPE);
        assert_eq!(device_fields["client_id"], id.to_string());

        let poll = transport
            .request(Request::Poll(id, "opaque+device"))
            .build()
            .unwrap();
        assert_eq!(poll.url().as_str(), TOKEN);
        assert_eq!(form(&poll)["device_code"], "opaque+device");
        assert_eq!(
            form(&poll)["grant_type"],
            "urn:ietf:params:oauth:grant-type:device_code"
        );

        let refresh = transport
            .request(Request::Refresh(id, "new+prior/token"))
            .build()
            .unwrap();
        assert_eq!(refresh.url().as_str(), TOKEN);
        assert_eq!(form(&refresh)["refresh_token"], "new+prior/token");
        assert_eq!(form(&refresh)["grant_type"], "refresh_token");

        let body = |request: &reqwest::blocking::Request| {
            serde_json::from_slice::<serde_json::Value>(request.body().unwrap().as_bytes().unwrap())
                .unwrap()
        };
        let user = transport
            .request(Request::XboxUser("mock-microsoft"))
            .build()
            .unwrap();
        assert_eq!(user.url().as_str(), XBOX);
        assert_eq!(body(&user)["Properties"]["RpsTicket"], "d=mock-microsoft");

        let minecraft = transport
            .request(Request::Minecraft("42", "mock-xsts"))
            .build()
            .unwrap();
        assert_eq!(minecraft.url().as_str(), MINECRAFT);
        assert_eq!(body(&minecraft)["identityToken"], "XBL3.0 x=42;mock-xsts");

        let profile = transport
            .request(Request::Profile("mock-mc"))
            .build()
            .unwrap();
        assert_eq!(profile.url().as_str(), PROFILE);
        assert_eq!(profile.method(), reqwest::Method::GET);

        let xsts = transport
            .request(Request::XboxTitle("mock-xbox"))
            .build()
            .unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(xsts.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(body["RelyingParty"], "rp://api.minecraftservices.com/");
        assert_eq!(body["Properties"]["UserTokens"][0], "mock-xbox");
        assert_eq!(xsts.url().as_str(), XSTS);

        let entitlements = transport
            .request(Request::Entitlements("mock-mc"))
            .build()
            .unwrap();
        assert_eq!(entitlements.url().as_str(), ENTITLEMENTS);
        let auth = &entitlements.headers()[reqwest::header::AUTHORIZATION];
        assert_eq!(auth, "Bearer mock-mc");
        assert!(auth.is_sensitive());
    }
}
