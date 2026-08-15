use std::env;

pub struct Config {
    pub host: String,
    pub port: u16,
    pub s3: S3Config,
}

pub struct S3Config {
    pub endpoint_host: String,
    pub access_key: String,
    pub secret_key: String,
    pub use_ssl: bool,
    pub region: String,
    pub submissions_bucket: String,
}

fn var(key: &str, fallback: &str) -> String {
    env::var(key).unwrap_or_else(|_| fallback.to_string())
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let endpoint_host =
            env::var("S3_ENDPOINT_HOST").map_err(|_| "S3_ENDPOINT_HOST is required".to_string())?;

        let port = var("ANALYZER_PORT", "8092")
            .parse::<u16>()
            .map_err(|e| format!("ANALYZER_PORT: {e}"))?;

        Ok(Self {
            host: var("ANALYZER_HOST", "0.0.0.0"),
            port,
            s3: S3Config {
                endpoint_host,
                access_key: var("S3_ACCESS_KEY_ID", ""),
                secret_key: var("S3_SECRET_ACCESS_KEY", ""),
                use_ssl: var("S3_USE_SSL", "true") == "true",
                region: var("S3_REGION", "us-east-1"),
                submissions_bucket: var("S3_SUBMISSIONS_BUCKET", "submissions"),
            },
        })
    }
}
