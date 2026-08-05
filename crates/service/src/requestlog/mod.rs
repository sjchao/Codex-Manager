#[path = "requestlog_clear.rs"]
pub(crate) mod clear;
#[path = "requestlog_error_list.rs"]
pub(crate) mod error_list;
#[path = "requestlog_list.rs"]
pub(crate) mod list;
#[path = "requestlog_summary.rs"]
pub(crate) mod summary;
#[path = "requestlog_today_summary.rs"]
pub(crate) mod today_summary;
pub(crate) mod image_assets;

#[cfg(test)]
mod image_assets_tests {
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use tiny_http::{Header, Response, Server};

    use super::image_assets::{
        cache_openai_image_results, clear_image_results, read_image_data_urls,
    };

    const PNG_BYTES: &[u8] = b"\x89PNG\r\n\x1a\n";
    const PNG_BASE64: &str = "iVBORw0KGgo=";

    fn test_db_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "codexmanager-request-log-image-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create test directory");
        dir.join("data").join("codexmanager.db")
    }

    #[test]
    fn caches_base64_images_below_the_database_data_directory() {
        let db_path = test_db_path("base64");
        fs::create_dir_all(db_path.parent().expect("database parent"))
            .expect("create data directory");
        let body = format!(r#"{{"data":[{{"b64_json":"{PNG_BASE64}"}}]}}"#);

        let assets = cache_openai_image_results(&db_path, "trc-image", body.as_bytes());

        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].mime_type, "image/png");
        assert_eq!(assets[0].byte_length, PNG_BYTES.len() as u64);
        assert!(assets[0].storage_key.starts_with("trc-image/"));
        let root = db_path
            .parent()
            .expect("database parent")
            .join("request-log-images");
        assert!(root.join(&assets[0].storage_key).is_file());

        let metadata = serde_json::to_string(&assets).expect("serialize asset metadata");
        let read = read_image_data_urls(&db_path, "trc-image", Some(&metadata))
            .expect("read cached image");
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].storage_key, assets[0].storage_key);
        assert_eq!(read[0].data_url, format!("data:image/png;base64,{PNG_BASE64}"));

        clear_image_results(&db_path, [Some(metadata)]).expect("clear cached image");
        assert!(!root.join(&assets[0].storage_key).exists());
        let _ = fs::remove_dir_all(root.parent().expect("test root"));
    }

    #[test]
    fn caches_http_image_urls_without_reusing_request_credentials() {
        let db_path = test_db_path("url");
        fs::create_dir_all(db_path.parent().expect("database parent"))
            .expect("create data directory");
        let server = Server::http("127.0.0.1:0").expect("start image server");
        let image_url = format!("http://{}/result.png", server.server_addr());
        let server_thread = thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(3))
                .expect("receive image request")
                .expect("image request exists");
            assert_eq!(request.url(), "/result.png");
            request
                .respond(
                    Response::from_data(PNG_BYTES)
                        .with_header(Header::from_bytes("Content-Type", "image/png").expect("header")),
                )
                .expect("respond image");
        });
        let body = serde_json::json!({ "data": [{ "url": image_url }] }).to_string();

        let assets = cache_openai_image_results(&db_path, "trc-url", body.as_bytes());

        server_thread.join().expect("join image server");
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].mime_type, "image/png");
        let root = db_path
            .parent()
            .expect("database parent")
            .join("request-log-images");
        assert!(root.join(&assets[0].storage_key).is_file());
        let _ = fs::remove_dir_all(root.parent().expect("test root"));
    }

    #[test]
    fn caches_data_uri_images_from_b64_json_and_url_fields() {
        let db_path = test_db_path("data-uri");
        fs::create_dir_all(db_path.parent().expect("database parent"))
            .expect("create data directory");
        let body = serde_json::json!({
            "data": [
                { "b64_json": format!("data:image/png;base64,{PNG_BASE64}") },
                { "url": format!("data:image/png;base64,{PNG_BASE64}") },
            ]
        })
        .to_string();

        let assets = cache_openai_image_results(&db_path, "trc-data-uri", body.as_bytes());

        assert_eq!(assets.len(), 2);
        assert!(assets.iter().all(|asset| asset.mime_type == "image/png"));
        let _ = fs::remove_dir_all(db_path.parent().expect("database parent").parent().expect("test root"));
    }

    #[test]
    fn caches_raw_image_response_bodies() {
        let db_path = test_db_path("raw");
        fs::create_dir_all(db_path.parent().expect("database parent"))
            .expect("create data directory");

        let assets = cache_openai_image_results(&db_path, "trc-raw", PNG_BYTES);

        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].mime_type, "image/png");
        assert_eq!(assets[0].byte_length, PNG_BYTES.len() as u64);
        let root = db_path
            .parent()
            .expect("database parent")
            .join("request-log-images");
        assert!(root.join(&assets[0].storage_key).is_file());
        let _ = fs::remove_dir_all(root.parent().expect("test root"));
    }

    #[test]
    fn caches_zstd_compressed_raw_image_response_bodies() {
        let db_path = test_db_path("zstd-raw");
        fs::create_dir_all(db_path.parent().expect("database parent"))
            .expect("create data directory");
        let compressed = zstd::stream::encode_all(std::io::Cursor::new(PNG_BYTES), 3)
            .expect("compress image response");

        let assets = cache_openai_image_results(&db_path, "trc-zstd-raw", &compressed);

        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].mime_type, "image/png");
        assert_eq!(assets[0].byte_length, PNG_BYTES.len() as u64);
        let _ = fs::remove_dir_all(db_path.parent().expect("database parent").parent().expect("test root"));
    }

    #[test]
    fn rejects_storage_keys_outside_the_image_root() {
        let db_path = test_db_path("path");
        let metadata = r#"[{"storageKey":"../outside.png","mimeType":"image/png","byteLength":8}]"#;

        let error = read_image_data_urls(&db_path, "trc-path", Some(metadata))
            .expect_err("reject parent-directory storage key");

        assert!(error.contains("storage key"));
        let _ = fs::remove_dir_all(db_path.parent().expect("database parent").parent().expect("test root"));
    }
}
