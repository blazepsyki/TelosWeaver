//! Documents signed by Reason: a compact JWS (RS256: RSASSA-PKCS1-v1_5 with SHA-256), checked here against Reason's
//! public keys, which are compiled in: verifying one never calls Reason. Only the relay list ([`crate::relays`]) is
//! taken this way. (TelosWeaver fork: Reason sign-in proofs, which the same keys signed, are not used.)

use anyhow::{Result, anyhow, bail};
use data_encoding::BASE64URL_NOPAD;
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, RsaPublicKeyComponents};
use serde::Deserialize;

/// The smallest RSA key trusted, in bits.
const MIN_BITS: usize = 2048;

/// Reason's signing public keys: key id, then the RSA modulus `n` and exponent `e` in base64url (as in a JWK). In
/// debug builds `REASON_NETWORK_TRUST_KEYS` (a JWKS) adds keys, for tests.
///
/// The cloud-access issuer's KMS key (`https://oidc.reasonmachines.com`), from
/// https://oidc.reasonmachines.com/.well-known/jwks.json on 2026-10-20. `the_production_keys_are_pinned` pins it to a
/// hash of its modulus.
const KEYS: &[(&str, &str, &str)] = &[(
    "reason-oidc-kms-2026-10",
    "s5x90QSBmHGl4adzvrjLvQ1ZRiwkDZGJ3pZs7CzPCxjKOBjoPoV6INAi4b8c3F4fMHH1T2BCXH9jX4wQjOCCzZBmB-E9Ch395WiqbLhyY2pH9V-ksbL9DGibS8xCoTT7EySB3FECzd_mHq-kQBCGxKnmBU0uETiPJLV276MzoBFsA8-s0PhsKqiQnUD3AKxHdDX-sWyI145QZasjZIT30Z8ixdRfrcUrLBMtWDhIqsOSP58i3_-3U-2gz_yG-7usgpyTcxCEU1IQGQtKBcJEVwd5QrbGgGE0w_uLxm2anfpT_eCZqOS_TWGYivVZ7TuqnxZgtON6kQgMNctSwprMDQ",
    "AQAB",
)];

/// Exactly these fields: anything else, `crit` included, is refused (RFC 7515: a `crit` not understood must be).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    alg: String,
    kid: String,
    typ: String,
}

#[cfg(any(test, debug_assertions))]
#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[cfg(any(test, debug_assertions))]
#[derive(Deserialize)]
struct Jwk {
    kty: String,
    kid: String,
    n: String,
    e: String,
}

type Key = RsaPublicKeyComponents<Vec<u8>>;

/// An RSA public key from its base64url modulus and exponent, if it is one of at least `MIN_BITS`.
fn key(n: &str, e: &str) -> Option<Key> {
    let decode = |v: &str| BASE64URL_NOPAD.decode(v.trim_end_matches('=').as_bytes()).ok();
    let strip = |v: Vec<u8>| v.into_iter().skip_while(|b| *b == 0).collect::<Vec<u8>>();
    let (n, e) = (strip(decode(n)?), strip(decode(e)?));
    let bits = n.first().map_or(0, |b| n.len() * 8 - b.leading_zeros() as usize);
    (bits >= MIN_BITS && !e.is_empty()).then_some(RsaPublicKeyComponents { n, e })
}

/// The keys a document may be signed with: the compiled-in ones and, in debug builds, those in
/// `REASON_NETWORK_TRUST_KEYS` (release builds trust only the compiled-in keys), and in tests a test key.
fn trusted() -> &'static [(String, Key)] {
    static KEYS_: std::sync::OnceLock<Vec<(String, Key)>> = std::sync::OnceLock::new();
    KEYS_.get_or_init(|| {
        #[allow(unused_mut)]
        let mut out: Vec<(String, Key)> =
            KEYS.iter().filter_map(|(kid, n, e)| Some((kid.to_string(), key(n, e)?))).collect();
        #[cfg(any(test, debug_assertions))]
        if let Some(jwks) = std::env::var_os("REASON_NETWORK_TRUST_KEYS")
            && let Ok(jwks) = serde_json::from_str::<Jwks>(&jwks.to_string_lossy())
        {
            out.extend(
                jwks.keys.into_iter().filter(|k| k.kty == "RSA").filter_map(|k| Some((k.kid, key(&k.n, &k.e)?))),
            );
        }
        #[cfg(test)]
        out.push((tests::KID.into(), tests::public(tests::signer())));
        out
    })
}

/// The claims of `jws`, a compact JWS of type `typ` (a `what`, in errors) that one of Reason's keys signed, unparsed.
/// What else they must say is the caller's to check.
pub fn verify_signed(jws: &str, typ: &str, what: &str, max_len: usize) -> Result<Vec<u8>> {
    signed_claims(jws, typ, what, max_len, trusted())
}

fn signed_claims(jws: &str, typ: &str, what: &str, max_len: usize, keys: &[(String, Key)]) -> Result<Vec<u8>> {
    let bad = || anyhow!("this {what} is damaged");
    if jws.len() > max_len {
        bail!("this {what} is too long");
    }
    let mut parts = jws.trim().split('.');
    let (Some(h), Some(c), Some(s), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return Err(bad());
    };
    let decode = |p: &str| BASE64URL_NOPAD.decode(p.as_bytes()).map_err(|_| bad());
    let header: Header = serde_json::from_slice(&decode(h)?).map_err(|_| bad())?;
    // RS256 only: never `none`, nor HS256 with the public key taken for a shared secret.
    if header.alg != "RS256" || header.typ != typ {
        bail!("not a Reason Network {what}");
    }
    let Some((_, key)) = keys.iter().find(|(kid, _)| *kid == header.kid) else {
        bail!("this {what} is signed with a key this version doesn't know");
    };
    if key.verify(&RSA_PKCS1_2048_8192_SHA256, format!("{h}.{c}").as_bytes(), &decode(s)?).is_err() {
        bail!("this {what}'s signature doesn't verify");
    }
    decode(c)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    use ring::signature::RsaKeyPair;
    use std::sync::OnceLock;

    pub const KID: &str = "test-1";
    const TYPE: &str = "test-doc";
    const MAX: usize = 4096;

    fn pkcs8(cell: &'static OnceLock<RsaKeyPair>, der: &[u8]) -> &'static RsaKeyPair {
        cell.get_or_init(|| RsaKeyPair::from_pkcs8(der).expect("a test key"))
    }

    /// The key test builds trust (see `trusted`), standing in for Reason's: a 2048-bit RSA key made for these tests.
    pub fn signer() -> &'static RsaKeyPair {
        static K: OnceLock<RsaKeyPair> = OnceLock::new();
        pkcs8(&K, include_bytes!("testdata/signer.pk8"))
    }

    /// A key nobody trusts.
    pub fn forger() -> &'static RsaKeyPair {
        static K: OnceLock<RsaKeyPair> = OnceLock::new();
        pkcs8(&K, include_bytes!("testdata/forger.pk8"))
    }

    pub fn public(k: &RsaKeyPair) -> Key {
        RsaPublicKeyComponents::from(k.public())
    }

    fn header() -> serde_json::Value {
        json!({"alg": "RS256", "kid": KID, "typ": TYPE})
    }

    fn claims() -> serde_json::Value {
        json!({"iss": "https://example.com", "n": 1})
    }

    /// The RS256 signature over `input`, in base64url.
    pub fn signature(key: &RsaKeyPair, input: &str) -> String {
        let mut sig = vec![0; key.public().modulus_len()];
        key.sign(&ring::signature::RSA_PKCS1_SHA256, &ring::rand::SystemRandom::new(), input.as_bytes(), &mut sig)
            .unwrap();
        BASE64URL_NOPAD.encode(&sig)
    }

    pub fn sign_with(key: &RsaKeyPair, header: serde_json::Value, claims: serde_json::Value) -> String {
        let enc = |v: serde_json::Value| BASE64URL_NOPAD.encode(&serde_json::to_vec(&v).unwrap());
        let input = format!("{}.{}", enc(header), enc(claims));
        format!("{input}.{}", signature(key, &input))
    }

    fn verify(jws: &str) -> Result<Vec<u8>> {
        verify_signed(jws, TYPE, "test document", MAX)
    }

    fn err(jws: &str) -> String {
        verify(jws).unwrap_err().to_string()
    }

    #[test]
    fn a_good_document_verifies() {
        let jws = sign_with(signer(), header(), claims());
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&verify(&jws).unwrap()).unwrap(), claims());
        assert!(verify(&format!("  {jws}\n")).is_ok());
    }

    #[test]
    fn rejects_bad_documents() {
        assert!(err(&sign_with(forger(), header(), claims())).contains("signature doesn't verify"));
        // A good signature over other claims.
        let good = sign_with(signer(), header(), claims());
        let other = sign_with(signer(), header(), json!({"iss": "https://evil.example"}));
        let mixed = format!(
            "{}.{}.{}",
            good.split('.').next().unwrap(),
            other.split('.').nth(1).unwrap(),
            good.split('.').nth(2).unwrap()
        );
        assert!(err(&mixed).contains("signature doesn't verify"));
        let unknown = sign_with(signer(), json!({"alg": "RS256", "kid": "other", "typ": TYPE}), claims());
        assert!(err(&unknown).contains("doesn't know"));
        let typ = sign_with(signer(), json!({"alg": "RS256", "kid": KID, "typ": "other-doc"}), claims());
        assert!(err(&typ).contains("not a Reason Network test document"));
        for damaged in ["", "a.b", "a.b.c.d", &good[..good.len() - 4]] {
            assert!(verify(damaged).is_err(), "{damaged:?}");
        }
    }

    #[test]
    fn only_trusted_keys_count() {
        let jws = sign_with(signer(), header(), claims());
        assert!(signed_claims(&jws, TYPE, "test document", MAX, &[]).is_err(), "no keys, nothing verifies");
        let keys = [(KID.to_string(), public(signer()))];
        assert!(signed_claims(&jws, TYPE, "test document", MAX, &keys).is_ok());
    }

    #[test]
    fn reads_a_jwks_key() {
        let p = public(signer());
        let (n, e) = (BASE64URL_NOPAD.encode(&p.n), BASE64URL_NOPAD.encode(&p.e));
        let jwks: Jwks = serde_json::from_value(
            json!({"keys": [{"kty": "RSA", "kid": KID, "n": n, "e": e, "alg": "RS256", "use": "sig"}]}),
        )
        .unwrap();
        let k = key(&jwks.keys[0].n, &jwks.keys[0].e).unwrap();
        assert_eq!((k.n, k.e), (p.n.clone(), p.e.clone()));
        // A leading zero byte (as some encoders write it) is the same key.
        let padded = BASE64URL_NOPAD.encode(&[&[0u8][..], &p.n].concat());
        assert_eq!(key(&padded, &e).unwrap().n, p.n);
        assert!(key("not base64!", &e).is_none());
        // Under 2048 bits isn't trusted.
        assert!(key(&BASE64URL_NOPAD.encode(&p.n[..128]), &e).is_none());
    }

    #[test]
    fn the_production_keys_are_pinned() {
        // SHA-256 of the modulus, as Reason's JWKS gave it on 2026-10-20: an accidental edit to `KEYS` fails here.
        let pins = [("reason-oidc-kms-2026-10", "c8a5d1e79ecc6c6056a7a11eac0b870a664f4b4ecb27392e7db4544c23e039c4")];
        assert_eq!(KEYS.len(), pins.len());
        for ((kid, n, e), (pinned_kid, pinned)) in KEYS.iter().zip(pins) {
            assert_eq!(*kid, pinned_kid);
            let k = key(n, e).expect("an RSA key of at least 2048 bits");
            let hash = data_encoding::HEXLOWER.encode(ring::digest::digest(&ring::digest::SHA256, &k.n).as_ref());
            assert_eq!(hash, pinned, "{kid}");
            assert_eq!(k.e, [1, 0, 1], "{kid}");
        }
        assert!(trusted().iter().any(|(k, _)| k == "reason-oidc-kms-2026-10"));
        assert!(!trusted().iter().any(|(k, _)| k == "reason-oidc-prod-2026-10-07"));
    }

    #[test]
    fn only_rs256_counts() {
        let c = claims();
        let enc = |v: &serde_json::Value| BASE64URL_NOPAD.encode(&serde_json::to_vec(v).unwrap());
        // HS256 keyed with Reason's public key (anyone has it), in the forms it gets published.
        let p = public(signer());
        let jwk =
            json!({"kty": "RSA", "kid": KID, "n": BASE64URL_NOPAD.encode(&p.n), "e": BASE64URL_NOPAD.encode(&p.e)});
        for secret in [p.n.clone(), serde_json::to_vec(&jwk).unwrap()] {
            let input = format!("{}.{}", enc(&json!({"alg": "HS256", "kid": KID, "typ": TYPE})), enc(&c));
            let tag = ring::hmac::sign(&ring::hmac::Key::new(ring::hmac::HMAC_SHA256, &secret), input.as_bytes());
            let jws = format!("{input}.{}", BASE64URL_NOPAD.encode(tag.as_ref()));
            assert!(err(&jws).contains("not a Reason Network test document"));
        }
        // Unsigned.
        for sig in ["", "AAAA"] {
            let jws = format!("{}.{}.{sig}", enc(&json!({"alg": "none", "kid": KID, "typ": TYPE})), enc(&c));
            assert!(err(&jws).contains("not a Reason Network test document"));
        }
        // Another RSA scheme's name, over a good RS256 signature.
        let good = sign_with(signer(), header(), c);
        let mut parts = good.split('.');
        let (_, body, sig) = (parts.next(), parts.next().unwrap(), parts.next().unwrap());
        let ps = format!("{}.{body}.{sig}", enc(&json!({"alg": "PS256", "kid": KID, "typ": TYPE})));
        assert!(verify(&ps).is_err());
    }

    #[test]
    fn refuses_a_huge_document_before_decoding_it() {
        let header = BASE64URL_NOPAD.encode(br#"{"alg":"RS256","kid":"test-1","typ":"test-doc"}"#);
        for big in [
            format!("{}.A.{}", "A".repeat(4 << 20), "A".repeat(342)),
            format!("{header}.{}.{}", "A".repeat(4 << 20), "A".repeat(342)),
        ] {
            let t = std::time::Instant::now();
            assert!(err(&big).contains("too long"));
            assert!(t.elapsed() < std::time::Duration::from_millis(50), "{:?}", t.elapsed());
        }
    }

    #[test]
    fn the_header_has_exactly_alg_kid_and_typ() {
        let c = claims();
        for header in [
            json!({"alg": "RS256", "kid": KID, "typ": TYPE, "crit": ["exp"], "exp": 1}),
            json!({"alg": "RS256", "kid": KID, "typ": TYPE, "jku": "https://evil.example/keys.json"}),
            json!({"alg": "RS256", "kid": KID}),
        ] {
            let jws = sign_with(signer(), header.clone(), c.clone());
            assert!(err(&jws).contains("damaged"), "{header}");
        }
        // A key twice is refused too.
        let h =
            BASE64URL_NOPAD.encode(format!(r#"{{"alg":"RS256","kid":"zzz","kid":"{KID}","typ":"{TYPE}"}}"#).as_bytes());
        let body = BASE64URL_NOPAD.encode(&serde_json::to_vec(&c).unwrap());
        let input = format!("{h}.{body}");
        let jws = format!("{input}.{}", signature(signer(), &input));
        assert!(verify(&jws).is_err());
    }
}
