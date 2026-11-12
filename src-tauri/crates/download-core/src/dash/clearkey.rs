use crate::CoreError;
use base64::engine::general_purpose::{STANDARD_NO_PAD, URL_SAFE_NO_PAD};
use base64::Engine;

/// Construit une requête de licence W3C ClearKey à partir d'un KID hexadécimal.
/// Corps JSON : { "kids": ["<base64url(kid)>"], "type": "temporary" }.
pub fn build_license_request(kid_hex: &str) -> Result<String, CoreError> {
    let kid = hex_to_bytes(kid_hex).ok_or_else(|| CoreError::Media(crate::i18n::tr("KID hex invalide", "Invalid hex KID").into()))?;
    let kid_b64 = URL_SAFE_NO_PAD.encode(kid);
    Ok(format!("{{\"kids\":[\"{kid_b64}\"],\"type\":\"temporary\"}}"))
}

/// Parse une réponse JWK ClearKey et renvoie la clé en hexadécimal.
/// { "keys": [ { "kty":"oct", "k":"<base64url>", "kid":"..." } ] }.
pub fn parse_jwk_response(json: &str) -> Result<String, CoreError> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| CoreError::Media(crate::trf!("JWK invalide: {e}", "Invalid JWK: {e}")))?;
    let k = v["keys"][0]["k"]
        .as_str()
        .ok_or_else(|| CoreError::Media(crate::i18n::tr("JWK sans clé `k`", "JWK without a `k` key").into()))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(k)
        .or_else(|_| STANDARD_NO_PAD.decode(k))
        .map_err(|e| CoreError::Media(crate::trf!("clé base64 invalide: {e}", "Invalid base64 key: {e}")))?;
    Ok(bytes_to_hex(&bytes))
}

fn hex_to_bytes(s: &str) -> Option<Vec<u8>> {
    let s = s.replace('-', "");
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

fn bytes_to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;

    #[test]
    fn license_request_encodes_kid() {
        let kid_hex = "000102030405060708090a0b0c0d0e0f";
        let req = build_license_request(kid_hex).unwrap();
        let expected_b64 = URL_SAFE_NO_PAD.encode((0u8..16).collect::<Vec<u8>>());
        assert!(req.contains(&expected_b64));
        assert!(req.contains("\"type\":\"temporary\""));
    }

    #[test]
    fn jwk_response_to_hex() {
        let key: Vec<u8> = (0u8..16).collect();
        let k_b64 = URL_SAFE_NO_PAD.encode(&key);
        let json = format!(r#"{{"keys":[{{"kty":"oct","k":"{k_b64}","kid":"x"}}]}}"#);
        assert_eq!(parse_jwk_response(&json).unwrap(), "000102030405060708090a0b0c0d0e0f");
    }
}
