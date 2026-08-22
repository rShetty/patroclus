use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::DecodePrivateKey;
use uuid::Uuid;

use crate::errors::{PatroclusError, Result};
use crate::token::{ActClaim, AgentClaims, IssueTokenParams};

pub struct TokenIssuer {
    issuer: String,
    encoding_key: EncodingKey,
    key_id: String,
    /// Public half of the signing key, published via the JWKS endpoint.
    public_key: rsa::RsaPublicKey,
}

impl TokenIssuer {
    pub fn ephemeral(issuer: &str) -> Result<Self> {
        use rsa::pkcs1::EncodeRsaPrivateKey;

        let mut rng = rand::thread_rng();
        let priv_key = rsa::RsaPrivateKey::new(&mut rng, 2048)
            .map_err(|e| PatroclusError::Crypto(format!("key generation failed: {}", e)))?;
        let pem = priv_key
            .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
            .map_err(|e| PatroclusError::Crypto(format!("private key encoding failed: {}", e)))?;
        Self::from_pem(pem.as_str(), issuer, "ephemeral")
    }

    pub fn from_pem(pem: &str, issuer: &str, key_id: &str) -> Result<Self> {
        let encoding_key = EncodingKey::from_rsa_pem(pem.as_bytes())
            .map_err(|e| PatroclusError::Crypto(format!("failed to load private key: {}", e)))?;
        let private_key = rsa::RsaPrivateKey::from_pkcs1_pem(pem)
            .or_else(|_| rsa::RsaPrivateKey::from_pkcs8_pem(pem))
            .map_err(|e| PatroclusError::Crypto(format!("failed to parse private key: {}", e)))?;
        Ok(TokenIssuer {
            issuer: issuer.to_string(),
            encoding_key,
            key_id: key_id.to_string(),
            public_key: private_key.to_public_key(),
        })
    }

    pub fn issue(&self, params: &IssueTokenParams) -> Result<(String, String)> {
        let now = Utc::now();
        let jti = Uuid::now_v7().to_string();
        let exp = params.expiry().timestamp();

        let claims = AgentClaims {
            iss: self.issuer.clone(),
            sub: params.subject.clone(),
            act: ActClaim {
                sub: params.agent_id.clone(),
                delegation_depth: params.delegation_depth,
                delegation_chain: params.delegation_chain.clone(),
            },
            scope: params.scopes.join(" "),
            aud: params.audience.clone(),
            exp,
            iat: now.timestamp(),
            jti: jti.clone(),
            constraints: params.constraints.clone(),
            cnf: None,
        };

        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.key_id.clone());

        let token = jsonwebtoken::encode(&header, &claims, &self.encoding_key)
            .map_err(|e| PatroclusError::Crypto(format!("token encoding failed: {}", e)))?;
        Ok((token, jti))
    }

    /// RFC 7517 JSON Web Key Set describing the public half of the signing
    /// key. Resource servers fetch this from `/.well-known/jwks.json` and
    /// validate issued tokens against it; `kid` matches the JWT header.
    pub fn public_jwks(&self) -> serde_json::Value {
        use rsa::traits::PublicKeyParts;

        let n = URL_SAFE_NO_PAD.encode(self.public_key.n().to_bytes_be());
        let e = URL_SAFE_NO_PAD.encode(self.public_key.e().to_bytes_be());
        serde_json::json!({
            "keys": [{
                "kty": "RSA",
                "use": "sig",
                "alg": "RS256",
                "kid": self.key_id,
                "n": n,
                "e": e,
            }]
        })
    }
}

pub fn generate_keypair(output_dir: &str) -> Result<()> {
    use rsa::pkcs1::{EncodeRsaPrivateKey, EncodeRsaPublicKey};

    let mut rng = rand::thread_rng();
    let bits = 2048;
    let priv_key = rsa::RsaPrivateKey::new(&mut rng, bits)
        .map_err(|e| PatroclusError::Crypto(format!("key generation failed: {}", e)))?;

    let priv_pem = priv_key
        .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
        .map_err(|e| PatroclusError::Crypto(format!("private key encoding failed: {}", e)))?;

    let pub_pem = priv_key
        .to_public_key()
        .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
        .map_err(|e| PatroclusError::Crypto(format!("public key encoding failed: {}", e)))?;

    std::fs::write(format!("{}/private.pem", output_dir), priv_pem.as_bytes())?;
    std::fs::write(format!("{}/public.pem", output_dir), pub_pem.as_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwks_describes_the_signing_key() {
        let issuer = TokenIssuer::ephemeral("http://test-issuer").unwrap();
        let jwks = issuer.public_jwks();

        let keys = jwks["keys"].as_array().unwrap();
        assert_eq!(keys.len(), 1);
        let key = &keys[0];
        assert_eq!(key["kty"], "RSA");
        assert_eq!(key["use"], "sig");
        assert_eq!(key["alg"], "RS256");
        assert_eq!(key["kid"], "ephemeral");
        // rsa generates 2048-bit keys with the standard 65537 exponent.
        assert_eq!(key["e"], "AQAB");
        assert!(!key["n"].as_str().unwrap().is_empty());
    }
}
