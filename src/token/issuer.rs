use chrono::Utc;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use uuid::Uuid;

use crate::errors::{PatroclusError, Result};
use crate::token::{AgentClaims, ActClaim, IssueTokenParams};

pub struct TokenIssuer {
    issuer: String,
    encoding_key: EncodingKey,
    key_id: String,
}

impl TokenIssuer {
    pub fn ephemeral(issuer: &str) -> Result<Self> {
        use rsa::pkcs1::EncodeRsaPrivateKey;
        use rsa::traits::PublicKeyParts;
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
        Ok(TokenIssuer {
            issuer: issuer.to_string(),
            encoding_key,
            key_id: key_id.to_string(),
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
