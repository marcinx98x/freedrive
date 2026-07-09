use crate::api::types::*;

use crate::auth_store::{save_auth, StoredAuth};

use crate::crypto::{self, generate_file_key, key_to_b64url};

use crate::db::{delete_pending_file_key, store_file_key, store_pending_file_key, DbHandle};

use crate::error::{AppError, AppResult};

use rand::RngCore;

use reqwest::multipart::{Form, Part};

use std::path::{Path, PathBuf};

use std::sync::Arc;

use std::time::Duration;

use parking_lot::RwLock;



#[derive(Clone)]

pub struct ApiClient {

    inner: Arc<RwLock<ClientInner>>,

}



struct ClientInner {

    server_url: String,

    access_token: String,

    refresh_token: String,

    http: reqwest::Client,

    upload_http: reqwest::Client,

}



struct PreparedUpload {

    temp_path: PathBuf,

    key: [u8; 32],

    name: String,

    mime: String,

    iv_b64: String,

    original_size: usize,

    folder_id: Option<String>,

}



impl ApiClient {

    pub fn from_auth(auth: &StoredAuth) -> Self {

        let server_url = auth.server_url.trim_end_matches('/').to_string();

        let http = reqwest::Client::builder()

            .timeout(Duration::from_secs(600))

            .build()

            .unwrap_or_else(|_| reqwest::Client::new());

        let upload_http = reqwest::Client::builder()

            .timeout(Duration::from_secs(120))

            .build()

            .unwrap_or_else(|_| reqwest::Client::new());

        Self {

            inner: Arc::new(RwLock::new(ClientInner {

                server_url,

                access_token: auth.access_token.clone(),

                refresh_token: auth.refresh_token.clone(),

                http,

                upload_http,

            })),

        }

    }



    pub fn server_url(&self) -> String {

        self.inner.read().server_url.clone()

    }



    fn api_url(&self, path: &str) -> String {

        let inner = self.inner.read();

        format!("{}/api/v1{}", inner.server_url, path)

    }



    async fn request_json<T: serde::de::DeserializeOwned>(

        &self,

        method: reqwest::Method,

        path: &str,

        body: Option<serde_json::Value>,

        retry: bool,

        rl_retries: u32,

    ) -> AppResult<T> {

        let url = self.api_url(path);

        let (access_token, http) = {

            let inner = self.inner.read();

            (inner.access_token.clone(), inner.http.clone())

        };



        let mut req = http.request(method.clone(), &url);

        req = req.header("Authorization", format!("Bearer {}", access_token));

        if let Some(ref b) = body {

            req = req.json(b);

        }



        let res = req.send().await?;

        if res.status() == reqwest::StatusCode::UNAUTHORIZED

            && !retry

            && path != "/auth/login"

            && path != "/auth/refresh"

        {

            if self.try_refresh().await? {

                return Box::pin(self.request_json(method, path, body, true, rl_retries)).await;

            }

            return Err(AppError::msg("session expired"));

        }



        if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS && rl_retries > 0 {

            tokio::time::sleep(Duration::from_millis(400)).await;

            return Box::pin(self.request_json(method, path, body, retry, rl_retries - 1)).await;

        }



        let status = res.status();

        let text = res.text().await?;

        if !status.is_success() {

            if let Ok(err) = serde_json::from_str::<ApiError>(&text) {

                return Err(AppError::msg(err.error));

            }

            return Err(AppError::msg(format!("request failed ({})", status)));

        }



        serde_json::from_str(&text).map_err(Into::into)

    }



    pub async fn try_refresh(&self) -> AppResult<bool> {

        let (url, refresh_token, http) = {

            let inner = self.inner.read();

            (

                format!("{}/api/v1/auth/refresh", inner.server_url),

                inner.refresh_token.clone(),

                inner.http.clone(),

            )

        };



        let res = http

            .post(&url)

            .json(&serde_json::json!({ "refresh_token": refresh_token }))

            .send()

            .await?;



        if !res.status().is_success() {

            return Ok(false);

        }



        let data: RefreshResponse = res.json().await?;

        {

            let mut inner = self.inner.write();

            inner.access_token = data.tokens.access_token.clone();

            inner.refresh_token = data.tokens.refresh_token.clone();

        }



        if let Ok(Some(mut auth)) = crate::auth_store::load_auth() {

            auth.access_token = data.tokens.access_token;

            auth.refresh_token = data.tokens.refresh_token;

            let _ = save_auth(&auth);

        }



        Ok(true)

    }



    pub async fn login(

        server_url: &str,

        email: &str,

        password: &str,

    ) -> AppResult<serde_json::Value> {

        let base = server_url.trim_end_matches('/');

        let http = reqwest::Client::new();

        let res = http

            .post(format!("{}/api/v1/auth/login", base))

            .json(&serde_json::json!({ "email": email, "password": password }))

            .send()

            .await?;



        let status = res.status();

        let text = res.text().await?;

        if !status.is_success() {

            if let Ok(err) = serde_json::from_str::<ApiError>(&text) {

                return Err(AppError::msg(err.error));

            }

            return Err(AppError::msg("login failed"));

        }

        serde_json::from_str(&text).map_err(Into::into)

    }



    pub async fn verify_2fa(challenge_id: &str, code: &str, server_url: &str) -> AppResult<LoginSuccess> {

        let base = server_url.trim_end_matches('/');

        let http = reqwest::Client::new();

        let res = http

            .post(format!("{}/api/v1/auth/verify-2fa", base))

            .json(&serde_json::json!({ "challenge_id": challenge_id, "code": code }))

            .send()

            .await?;



        let status = res.status();

        let text = res.text().await?;

        if !status.is_success() {

            if let Ok(err) = serde_json::from_str::<ApiError>(&text) {

                return Err(AppError::msg(err.error));

            }

            return Err(AppError::msg("2FA verification failed"));

        }

        serde_json::from_str(&text).map_err(Into::into)

    }



    pub async fn get_me(&self) -> AppResult<User> {
        self.request_json(reqwest::Method::GET, "/me", None, false, 2)
            .await
    }

    pub async fn get_my_storage(&self) -> AppResult<StorageInfo> {
        self.request_json(reqwest::Method::GET, "/me/storage", None, false, 2)
            .await
    }

    pub async fn get_my_drive_root(&self) -> AppResult<FolderContents> {
        self.request_json(reqwest::Method::GET, "/folders/root", None, false, 2)
            .await
    }

    pub async fn get_shared_with_me(&self) -> AppResult<Vec<SharedItem>> {
        let resp: SharedWithMeResponse = self
            .request_json(reqwest::Method::GET, "/shares/with-me", None, false, 2)
            .await?;
        Ok(resp.items)
    }

    pub async fn logout(&self) -> AppResult<()> {

        let refresh_token = self.inner.read().refresh_token.clone();

        let _: serde_json::Value = self

            .request_json(

                reqwest::Method::POST,

                "/auth/logout",

                Some(serde_json::json!({ "refresh_token": refresh_token })),

                false,

                2,

            )

            .await?;

        Ok(())

    }



    pub async fn register_computer(&self, name: &str, hostname: &str) -> AppResult<Computer> {

        self.request_json(

            reqwest::Method::POST,

            "/computers/register",

            Some(serde_json::json!({ "name": name, "hostname": hostname })),

            false,

            2,

        )

        .await

    }



    pub async fn heartbeat(&self, computer_id: &str) -> AppResult<Computer> {

        self.request_json(

            reqwest::Method::POST,

            &format!("/computers/{}/heartbeat", computer_id),

            None,

            false,

            2,

        )

        .await

    }



    pub async fn create_folder(

        &self,

        name: &str,

        parent_id: Option<&str>,

    ) -> AppResult<Folder> {

        self.request_json(

            reqwest::Method::POST,

            "/folders",

            Some(serde_json::json!({

                "name": name,

                "parent_id": parent_id

            })),

            false,

            2,

        )

        .await

    }



    pub async fn resolve_folder_by_name(

        &self,

        parent_id: &str,

        name: &str,

    ) -> AppResult<Option<Folder>> {

        let contents = self.get_folder_contents(parent_id).await?;

        Ok(contents

            .folders

            .into_iter()

            .find(|f| f.name == name))

    }



    pub async fn create_or_resolve_folder(

        &self,

        name: &str,

        parent_id: Option<&str>,

    ) -> AppResult<Folder> {

        match self.create_folder(name, parent_id).await {

            Ok(folder) => Ok(folder),

            Err(_) => {

                let parent = parent_id

                    .ok_or_else(|| AppError::msg("parent folder required"))?;

                self.resolve_folder_by_name(parent, name)

                    .await?

                    .ok_or_else(|| AppError::msg(format!("folder not found: {}", name)))

            }

        }

    }



    pub async fn get_folder_contents(&self, folder_id: &str) -> AppResult<FolderContents> {

        self.request_json(

            reqwest::Method::GET,

            &format!("/folders/{}", folder_id),

            None,

            false,

            2,

        )

        .await

    }



    fn encrypt_to_temp_file(

        local_path: &Path,

        name: &str,

        folder_id: Option<&str>,

        existing_key: Option<[u8; 32]>,

    ) -> AppResult<PreparedUpload> {

        let plaintext = std::fs::read(local_path)?;

        let original_size = plaintext.len();

        let mime = mime_guess::from_path(local_path)

            .first_or_octet_stream()

            .to_string();



        let key = existing_key.unwrap_or_else(generate_file_key);

        let (ciphertext, iv) = crypto::encrypt_file(&plaintext, &key)?;



        let mut temp_path = std::env::temp_dir();

        let mut suffix = [0u8; 8];

        rand::thread_rng().fill_bytes(&mut suffix);

        temp_path.push(format!("freedrive-upload-{}.enc", hex::encode(suffix)));

        std::fs::write(&temp_path, &ciphertext)?;



        Ok(PreparedUpload {

            temp_path,

            key,

            name: name.to_string(),

            mime,

            iv_b64: crypto::iv_to_base64(&iv),

            original_size,

            folder_id: folder_id.map(|s| s.to_string()),

        })

    }



    fn remove_temp_upload(path: &Path) {

        let _ = std::fs::remove_file(path);

    }



    pub async fn upload_file(

        &self,

        db: &DbHandle,

        local_path: &Path,

        name: &str,

        folder_id: &str,

    ) -> AppResult<(FileRecord, [u8; 32])> {

        self.upload_multipart_with_retry(

            Some(db),

            "/files/upload",

            local_path,

            name,

            Some(folder_id),

            None,

        )

        .await

    }



    pub async fn update_file_content(

        &self,

        file_id: &str,

        local_path: &Path,

        name: &str,

        existing_key: Option<[u8; 32]>,

    ) -> AppResult<(FileRecord, [u8; 32])> {

        self.upload_multipart_with_retry(

            None,

            &format!("/files/{}/content", file_id),

            local_path,

            name,

            None,

            existing_key,

        )

        .await

    }



    async fn upload_multipart_with_retry(

        &self,

        db: Option<&DbHandle>,

        path: &str,

        local_path: &Path,

        name: &str,

        folder_id: Option<&str>,

        existing_key: Option<[u8; 32]>,

    ) -> AppResult<(FileRecord, [u8; 32])> {

        let mut auth_retry = false;

        let mut rl_retries = 2u32;



        loop {

            let file_size = std::fs::metadata(local_path)

                .map(|m| m.len())

                .unwrap_or(0);

            let prep_timeout = crate::blocking::upload_prep_timeout(file_size);

            let http_timeout = crate::blocking::upload_http_timeout(file_size);



            let prepared = {

                let local_path = local_path.to_path_buf();

                let name = name.to_string();

                let folder_id = folder_id.map(|s| s.to_string());



                crate::blocking::run_blocking_with_timeout_async(prep_timeout, move || {

                    Self::encrypt_to_temp_file(

                        &local_path,

                        &name,

                        folder_id.as_deref(),

                        existing_key,

                    )

                })

                .await?



            };



            if let (Some(db), Some(folder_id)) = (db, prepared.folder_id.as_deref()) {

                if existing_key.is_none() {

                    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;

                    store_pending_file_key(

                        &conn,

                        folder_id,

                        &prepared.name,

                        &key_to_b64url(&prepared.key),

                    )?;

                }

            }



            crate::sync::log::sync_log(format!(

                "encrypt ok {} ({} bytes)",

                prepared.name,

                prepared.original_size

            ));



            crate::sync::log::sync_log(format!(

                "http start {} ({})",

                prepared.name,

                path

            ));



            let upload_result: AppResult<crate::api::types::FileRecord> = self

                .multipart_stream_once(path, &prepared, http_timeout)

                .await;



            Self::remove_temp_upload(&prepared.temp_path);



            match upload_result {

                Ok(rec) => {

                    let key_b64 = key_to_b64url(&prepared.key);

                    if let Some(db) = db {

                        if let Ok(conn) = db.lock() {

                            let _ = store_file_key(&conn, &rec.id, &key_b64);

                            if let Some(folder_id) = prepared.folder_id.as_deref() {

                                if existing_key.is_none() {

                                    let _ =

                                        delete_pending_file_key(&conn, folder_id, &prepared.name);

                                }

                            }

                        }

                    }

                    if let Some(auth) = crate::auth_store::load_auth().ok().flatten() {
                        if let Ok(user) =
                            serde_json::from_str::<serde_json::Value>(&auth.user_json)
                        {
                            if let Some(uid) = user.get("id").and_then(|v| v.as_str()) {
                                if let Some(uek) = crate::account_crypto::get_uek(uid) {
                                    let _ = crate::account_crypto::push_file_key(
                                        self,
                                        &uek,
                                        &rec.id,
                                        &key_b64,
                                    )
                                    .await;
                                }
                            }
                        }
                    }

                    crate::sync::log::sync_log(format!("http ok {}", prepared.name));

                    return Ok((rec, prepared.key));

                }

                Err(e) => {

                    let msg = e.to_string();

                    if !auth_retry && msg.contains("session expired") {

                        if self.try_refresh().await? {

                            auth_retry = true;

                            continue;

                        }

                    }

                    if rl_retries > 0 && msg.contains("rate limit") {

                        rl_retries -= 1;

                        tokio::time::sleep(Duration::from_millis(400)).await;

                        continue;

                    }

                    if let (Some(db), Some(folder_id)) = (db, prepared.folder_id.as_deref()) {

                        if existing_key.is_none() {

                            if let Ok(conn) = db.lock() {

                                let _ = delete_pending_file_key(&conn, folder_id, &prepared.name);

                            }

                        }

                    }

                    return Err(e);

                }

            }

        }

    }



    pub async fn download_file(

        &self,

        file_id: &str,

        key_b64url: Option<&str>,

    ) -> AppResult<Vec<u8>> {

        let mut auth_retry = false;

        let mut rl_retries = 2u32;



        loop {

            match self.download_file_once(file_id, key_b64url).await {

                Ok(bytes) => return Ok(bytes),

                Err(e) => {

                    let msg = e.to_string();

                    if !auth_retry && msg.contains("session expired") {

                        if self.try_refresh().await? {

                            auth_retry = true;

                            continue;

                        }

                    }

                    if rl_retries > 0 && msg.contains("rate limit") {

                        rl_retries -= 1;

                        tokio::time::sleep(Duration::from_millis(400)).await;

                        continue;

                    }

                    return Err(e);

                }

            }

        }

    }



    async fn download_file_once(

        &self,

        file_id: &str,

        key_b64url: Option<&str>,

    ) -> AppResult<Vec<u8>> {

        let url = self.api_url(&format!("/files/{}/download", file_id));

        let (access_token, http) = {

            let inner = self.inner.read();

            (inner.access_token.clone(), inner.http.clone())

        };

        let res = http

            .get(&url)

            .header("Authorization", format!("Bearer {}", access_token))

            .send()

            .await?;



        if res.status() == reqwest::StatusCode::UNAUTHORIZED {

            if self.try_refresh().await? {

                return Err(AppError::msg("session expired"));

            }

            return Err(AppError::msg("session expired"));

        }



        if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {

            return Err(AppError::msg("rate limit exceeded"));

        }



        if !res.status().is_success() {

            return Err(AppError::msg("download failed"));

        }



        let iv_header = res

            .headers()

            .get("x-file-iv")

            .and_then(|v| v.to_str().ok())

            .unwrap_or("")

            .to_string();

        let bytes = res.bytes().await?.to_vec();



        if iv_header.is_empty() {

            return Ok(bytes);

        }



        let key_b64url = key_b64url

            .ok_or_else(|| AppError::msg("missing encryption key"))?;

        let key = crypto::key_from_b64url(key_b64url)?;

        let iv = crypto::iv_from_base64(&iv_header)?;

        crypto::decrypt_file(&bytes, &key, &iv)

    }



    async fn multipart_stream_once<T: serde::de::DeserializeOwned>(

        &self,

        path: &str,

        prepared: &PreparedUpload,

        timeout: Duration,

    ) -> AppResult<T> {

        let url = self.api_url(path);

        let access_token = self.inner.read().access_token.clone();

        let http = self.inner.read().upload_http.clone();



        let file = tokio::fs::File::open(&prepared.temp_path).await?;



        let part = Part::stream(file)

            .file_name(prepared.name.clone())

            .mime_str(&prepared.mime)

            .map_err(|e| AppError::msg(e.to_string()))?;



        let mut form = Form::new()

            .part("file", part)

            .text("name", prepared.name.clone())

            .text("mime_type", prepared.mime.clone())

            .text("iv", prepared.iv_b64.clone())

            .text("original_size", prepared.original_size.to_string());



        if let Some(fid) = &prepared.folder_id {

            form = form.text("folder_id", fid.clone());

        }



        let res = http

            .post(&url)

            .header("Authorization", format!("Bearer {}", access_token))

            .multipart(form)

            .timeout(timeout)

            .send()

            .await?;



        if res.status() == reqwest::StatusCode::UNAUTHORIZED {

            if self.try_refresh().await? {

                return Err(AppError::msg("session expired"));

            }

            return Err(AppError::msg("session expired"));

        }



        if res.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {

            return Err(AppError::msg("rate limit exceeded"));

        }



        let status = res.status();

        let text = res.text().await?;

        if !status.is_success() {

            if let Ok(err) = serde_json::from_str::<ApiError>(&text) {

                return Err(AppError::msg(err.error));

            }

            return Err(AppError::msg(format!("upload failed ({})", status)));

        }

        serde_json::from_str(&text).map_err(Into::into)

    }

    pub async fn get_crypto_account(
        &self,
    ) -> AppResult<serde_json::Value> {
        self.request_json(reqwest::Method::GET, "/crypto/account", None, false, 2)
            .await
    }

    pub async fn setup_crypto_account(
        &self,
        key_salt: &[u8],
        wrapped_uek: &str,
        wrapped_uek_recovery: Option<&str>,
    ) -> AppResult<()> {
        let mut body = serde_json::json!({
            "key_salt": key_salt,
            "wrapped_uek": wrapped_uek,
        });
        if let Some(recovery) = wrapped_uek_recovery {
            body["wrapped_uek_recovery"] = serde_json::Value::String(recovery.to_string());
        }
        let _: serde_json::Value = self
            .request_json(reqwest::Method::POST, "/crypto/account", Some(body), false, 2)
            .await?;
        Ok(())
    }

    pub async fn list_encryption_keys(
        &self,
        since: &str,
    ) -> AppResult<serde_json::Value> {
        let path = if since.is_empty() {
            "/encryption-keys".to_string()
        } else {
            format!("/encryption-keys?since={}", urlencoding::encode(since))
        };
        self.request_json(reqwest::Method::GET, &path, None, false, 2)
            .await
    }

    pub async fn bulk_put_encryption_keys(
        &self,
        keys: std::collections::HashMap<String, String>,
    ) -> AppResult<serde_json::Value> {
        self.request_json(
            reqwest::Method::POST,
            "/encryption-keys/bulk",
            Some(serde_json::json!({ "keys": keys })),
            false,
            2,
        )
        .await
    }

    pub async fn get_file_encryption_key(
        &self,
        file_id: &str,
    ) -> AppResult<serde_json::Value> {
        self.request_json(
            reqwest::Method::GET,
            &format!("/files/{}/encryption-key", file_id),
            None,
            false,
            2,
        )
        .await
    }

    pub async fn put_file_encryption_key(
        &self,
        file_id: &str,
        wrapped_file_key: &str,
    ) -> AppResult<()> {
        let _: serde_json::Value = self
            .request_json(
                reqwest::Method::PUT,
                &format!("/files/{}/encryption-key", file_id),
                Some(serde_json::json!({ "wrapped_file_key": wrapped_file_key })),
                false,
                2,
            )
            .await?;
        Ok(())
    }

}



#[allow(dead_code)]
pub fn file_key_b64url(key: &[u8; 32]) -> String {

    key_to_b64url(key)

}


