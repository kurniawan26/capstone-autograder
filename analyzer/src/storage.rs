use aws_credential_types::Credentials;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::{BehaviorVersion, Region};

use crate::config::S3Config;

pub struct Store {
    client: Client,
    bucket: String,
}

impl Store {
    pub fn new(cfg: &S3Config) -> Self {
        let scheme = if cfg.use_ssl { "https" } else { "http" };
        let credentials = Credentials::new(
            cfg.access_key.clone(),
            cfg.secret_key.clone(),
            None,
            None,
            "static",
        );

        let conf = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(cfg.region.clone()))
            .endpoint_url(format!("{scheme}://{}", cfg.endpoint_host))
            .credentials_provider(credentials)
            .force_path_style(true)
            .build();

        Self {
            client: Client::from_conf(conf),
            bucket: cfg.submissions_bucket.clone(),
        }
    }

    pub async fn fetch(&self, key: &str) -> Result<Vec<u8>, String> {
        let object = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| format!("get {}/{}: {}", self.bucket, key, service_error(e)))?;

        let bytes = object
            .body
            .collect()
            .await
            .map_err(|e| format!("read body of {}/{}: {e}", self.bucket, key))?;

        Ok(bytes.into_bytes().to_vec())
    }
}

fn service_error<E: std::fmt::Debug, R: std::fmt::Debug>(
    err: aws_sdk_s3::error::SdkError<E, R>,
) -> String {
    match &err {
        aws_sdk_s3::error::SdkError::ServiceError(inner) => format!("{:?}", inner.err()),
        other => format!("{other:?}"),
    }
}
