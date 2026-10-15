use std::{
    env,
    fs::{self, OpenOptions},
    io::{self, Write},
};

use crate::{
    fs::ContentDigest,
    project_host::{StartupContentClosureReceipt, StartupContentMemberKind},
};

pub use crate::project_workflow::ProjectExportBinaryTarget as ProductObservationTarget;

const OBSERVATION_ATTEMPT_ENV: &str = "NARA_STARTER_OBSERVATION_ATTEMPT";
const OBSERVATION_FILE_ENV: &str = "NARA_STARTER_OBSERVATION_FILE";

/// Records the exact admitted closure when the native export adapter requests an observation.
#[doc(hidden)]
pub fn record_product_observation(
    target: ProductObservationTarget,
    closure: &StartupContentClosureReceipt,
) -> io::Result<()> {
    let (attempt, output) = match (
        env::var_os(OBSERVATION_ATTEMPT_ENV),
        env::var_os(OBSERVATION_FILE_ENV),
    ) {
        (None, None) => return Ok(()),
        (Some(attempt), Some(output)) => (attempt, output),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "product observation requires both attempt and output",
            ));
        }
    };
    let attempt = attempt.into_string().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "product observation attempt must be Unicode",
        )
    })?;
    if attempt.is_empty()
        || attempt.len() > 128
        || !attempt
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "product observation attempt is invalid",
        ));
    }
    let executable = ContentDigest::of_bytes(&fs::read(env::current_exe()?)?);
    let manifest = closure.manifest_digest();
    let content = closure.content_digest();
    let closure_plan = closure.closure_plan_digest();
    let bundle = closure
        .members()
        .iter()
        .find(|member| member.kind() == StartupContentMemberKind::AuthoringSchemaBundle)
        .map(crate::project_host::StartupContentMemberReceipt::digest)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "product observation requires the admitted authoring schema bundle",
            )
        })?;
    let profile = product_profile(target);
    let value = serde_json::json!({
        "version":2,"attempt":attempt,"target":target.cargo_target_name(),"profile":profile,
        "executable_length":executable.length(),"executable_blake3":digest_hex(executable),
        "manifest_length":manifest.length(),"manifest_blake3":digest_hex(manifest),
        "content_length":content.length(),"content_blake3":digest_hex(content),
        "closure_plan_length":closure_plan.length(),"closure_plan_blake3":digest_hex(closure_plan),
        "schema_fingerprint":closure.schema_fingerprint().to_string(),
        "authoring_bundle_length":bundle.length(),"authoring_bundle_blake3":digest_hex(bundle),
    });
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    serde_json::to_writer(&mut file, &value).map_err(io::Error::other)?;
    file.write_all(b"\n")
}

pub(super) const fn product_profile(target: ProductObservationTarget) -> &'static str {
    match target {
        ProductObservationTarget::Headless => "default",
        ProductObservationTarget::Desktop => "desktop",
    }
}

pub(super) fn digest_hex(digest: ContentDigest) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in digest.as_bytes() {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}
