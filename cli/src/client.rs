pub(crate) struct Blob {
    pub(crate) name: String,
    pub(crate) data: bytes::Bytes,
}

pub(crate) struct Client {
    base_url: reqwest::Url,
    http_client: reqwest::Client,
    root_key: scrooje_crypto::RootKey,
}

impl Client {
    const MAX_RETRIES: u64 = 5;
    const DEFAULT_RETRY_DELAY_MILLIS: u64 = 1_000;
    const DEFAULT_TIMEOUT_SECS: u64 = 20;
    const CLIENT_JWT_EXP_DURATION: i64 = Self::DEFAULT_TIMEOUT_SECS as i64 + 10;

    pub(crate) fn new(
        base_url: reqwest::Url,
        http_client: reqwest::Client,
        root_key: scrooje_crypto::RootKey,
    ) -> Client {
        Client {
            base_url,
            http_client,
            root_key,
        }
    }

    async fn send_request(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<bytes::Bytes, ClientError> {
        #[derive(serde::Deserialize)]
        struct ErrorResponsePayload {
            error: String,
            // Retry after in milliseconds.
            after: Option<u64>,
        }

        let mut delay = None;
        let mut last_err = None;
        for attempt in 0..Self::MAX_RETRIES {
            if attempt > 0 {
                if let Some(err) = last_err.as_ref() {
                    log::warn!("attempt #{attempt} to request api failed: {err}");
                }
                let duration = std::time::Duration::from_millis(
                    delay.unwrap_or_else(|| attempt * Self::DEFAULT_RETRY_DELAY_MILLIS),
                );
                tokio::time::sleep(duration).await;
            }

            let response = match request.try_clone().unwrap().send().await {
                Ok(response) => response,
                Err(err) => {
                    delay = None;
                    last_err.replace(err.into());
                    continue;
                }
            };

            let status = response.status();
            if status.is_server_error() {
                delay = None;
                last_err.replace(response.error_for_status().unwrap_err().into());
                continue;
            }
            let bytes = match response.bytes().await {
                Ok(bytes) => bytes,
                Err(err) => {
                    delay = None;
                    last_err.replace(err.into());
                    continue;
                }
            };
            if status.is_client_error() {
                let payload: ErrorResponsePayload =
                    serde_json::from_slice(&bytes).unwrap_or_else(|_| ErrorResponsePayload {
                        error: "<unknown error>".to_owned(),
                        after: None,
                    });
                if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    delay = payload.after;
                    last_err.replace(ClientError::RateLimited);
                    continue;
                }
                return Err(ClientError::BadRequest(status, payload.error));
            }

            return Ok(bytes);
        }

        if let Some(err) = last_err {
            return Err(err);
        }

        unreachable!()
    }

    pub(crate) async fn login(&self, username: &str) -> Result<String, ClientError> {
        #[derive(serde::Deserialize)]
        struct LoginResponsePayload {
            jwt: String,
        }

        let mut url = self.base_url.clone();
        url.set_path("/api/v1/auth/login");
        url.query_pairs_mut().append_pair("username", username);

        let body = serde_json::to_vec(&serde_json::json!({ "extended_session": false }))?;

        let signing_keys = self.root_key.signing_keys();
        let exp_after = std::time::Duration::from_secs(
            Self::CLIENT_JWT_EXP_DURATION
                .try_into()
                .expect("30 is a valid u64"),
        );
        let client_jwt =
            signing_keys.sign_request("POST", url.as_str(), Some(body.as_slice()), exp_after);
        let request = self
            .http_client
            .post(url)
            // Setting timeout to the lifetime of the client-issued JWT.
            .timeout(exp_after)
            .header("content-type", "application/json")
            .header("x-client-jwt", client_jwt)
            .body(body);

        let bytes = self.send_request(request).await?;
        let payload: LoginResponsePayload = serde_json::from_slice(&bytes)?;

        Ok(payload.jwt)
    }

    pub(crate) async fn get_book_secret_key(
        &self,
        jwt: &str,
        book_uuid: uuid::Uuid,
    ) -> Result<scrooje_crypto::BookSecretKey, ClientError> {
        #[derive(serde::Deserialize)]
        struct BooksResponsePayload {
            books: Vec<Book>,
        }
        #[derive(serde::Deserialize)]
        struct Book {
            uuid: uuid::Uuid,
            root_key: String,
        }

        let mut url = self.base_url.clone();
        url.set_path("/api/v1/books");
        let request = self
            .http_client
            .get(url)
            .timeout(std::time::Duration::from_secs(Self::DEFAULT_TIMEOUT_SECS))
            .bearer_auth(jwt);

        let bytes = self.send_request(request).await?;
        let payload: BooksResponsePayload = serde_json::from_slice(&bytes)?;
        let book = payload
            .books
            .iter()
            .find(|book| book.uuid == book_uuid)
            .ok_or(ClientError::NoBooksMatched(book_uuid))?;

        let encrypted_root_key = scrooje_crypto::decode_hex(&book.root_key)?;
        let encryption_key = self.root_key.book_encryption_key(book_uuid);
        let book_secret_key = scrooje_crypto::BookSecretKey::from_slice(
            encryption_key
                .decrypt(&encrypted_root_key)
                .map_err(ClientError::Decrypt)?
                .as_bytes(),
        )
        .map_err(ClientError::MalformedBookSecretKey)?;
        Ok(book_secret_key)
    }

    pub(crate) async fn get_blobs(
        &self,
        jwt: &str,
        book_uuid: uuid::Uuid,
    ) -> Result<Vec<Blob>, ClientError> {
        #[derive(serde::Deserialize)]
        struct GenerationsResponsePayload {
            generations: Vec<Generation>,
        }
        #[derive(serde::Deserialize)]
        struct Generation {
            name: String,
            number: u64,
        }

        let mut url = self.base_url.clone();
        url.set_path(&format!("/api/v1/books/{book_uuid}/generations"));
        let request = self
            .http_client
            .get(url)
            .timeout(std::time::Duration::from_secs(Self::DEFAULT_TIMEOUT_SECS))
            .bearer_auth(jwt);

        let bytes = self.send_request(request).await?;
        let payload: GenerationsResponsePayload = serde_json::from_slice(&bytes)?;

        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(5));
        let tasks = payload.generations.into_iter().map(|generation| {
            let mut url = self.base_url.clone();
            url.set_path(&format!(
                "/api/v1/books/{book_uuid}/blobs/{}",
                generation.name
            ));
            url.set_query(Some(&format!("generation={}", generation.number)));
            let request = self
                .http_client
                .get(url)
                .timeout(std::time::Duration::from_secs(Self::DEFAULT_TIMEOUT_SECS))
                .bearer_auth(jwt);

            let semaphore_clone = semaphore.clone();
            async move {
                let _permit = semaphore_clone
                    .acquire()
                    .await
                    .expect("semaphore should acquire");
                self.send_request(request).await.map(|bytes| Blob {
                    name: generation.name,
                    data: bytes,
                })
            }
        });

        futures::future::try_join_all(tasks).await
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ClientError {
    #[error("request failed with status {0}: {1}")]
    BadRequest(reqwest::StatusCode, String),
    #[error("decryption failed: {0}")]
    Decrypt(scrooje_crypto::DecryptionError),
    #[error("hex decoding failed: {0}")]
    HexError(#[from] scrooje_crypto::HexError),
    #[error("json error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("no books matched uuid: {0}")]
    NoBooksMatched(uuid::Uuid),
    #[error("malformed book secret key: {0}")]
    MalformedBookSecretKey(scrooje_crypto::BookSecretKeyError),
    #[error("http request rate limited")]
    RateLimited,
    #[error("http request failed: {0}")]
    ReqwestError(#[from] reqwest::Error),
}
