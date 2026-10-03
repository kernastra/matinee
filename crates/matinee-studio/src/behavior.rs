//! Service behavior with a fake vault, scripted HTTP, and a scripted process.
//! Nothing here talks to fal, Codex, or the OS keyring.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use matinee_secrets::{CredentialStore, MemoryStore, Secret};
use tokio::sync::Notify;

use crate::codex::save_codex_path;
use crate::error::StudioError;
use crate::http::{StudioHttp, StudioRequest, StudioResponse};
use crate::model::{ArtworkKind, GenerationRequest, ImageProvider};
use crate::paths::StudioPaths;
use crate::process::{ProcessError, ProcessOutput, ProcessRunner, ProcessSpec};
use crate::service::Studio;

const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

struct ScriptHttp {
    responses: Mutex<Vec<Result<StudioResponse, StudioError>>>,
    seen: Arc<Mutex<Vec<StudioRequest>>>,
}

impl ScriptHttp {
    fn new(responses: Vec<Result<StudioResponse, StudioError>>) -> Self {
        Self {
            responses: Mutex::new(responses),
            seen: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn seen(&self) -> Arc<Mutex<Vec<StudioRequest>>> {
        Arc::clone(&self.seen)
    }
}

impl StudioHttp for ScriptHttp {
    async fn send(&self, request: StudioRequest) -> Result<StudioResponse, StudioError> {
        self.seen.lock().expect("http log").push(request);
        self.responses.lock().expect("http script").remove(0)
    }
}

struct ScriptProcess {
    result: Mutex<Option<Result<ProcessOutput, ProcessError>>>,
    started: Arc<Notify>,
    release: Arc<Notify>,
    calls: Arc<AtomicUsize>,
    saw_shell: Arc<Mutex<bool>>,
}

impl ProcessRunner for ScriptProcess {
    async fn run(&self, spec: ProcessSpec) -> Result<ProcessOutput, ProcessError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let shell = spec.args.iter().any(|arg| {
            matches!(arg.as_str(), "sh" | "cmd" | "powershell" | "pwsh") || arg.contains("sh -c")
        });
        *self.saw_shell.lock().expect("shell flag") = shell;
        self.started.notify_one();
        self.release.notified().await;
        self.result
            .lock()
            .expect("process result")
            .take()
            .unwrap_or(Err(ProcessError::Timeout))
    }
}

struct Scratch {
    root: PathBuf,
    paths: StudioPaths,
}

impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "matinee-studio-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let home = root.join("home");
        let data = root.join("data");
        let pictures = root.join("pictures");
        std::fs::create_dir_all(home.join("media/movies")).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        std::fs::create_dir_all(&pictures).unwrap();
        Self {
            paths: StudioPaths::new(home, data, Some(pictures)),
            root,
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn prompt() -> String {
    "A quiet theatrical poster with a single figure in warm light.".into()
}

fn request(provider: ImageProvider, prompt: String) -> GenerationRequest {
    GenerationRequest {
        provider,
        prompt,
        reference_urls: Vec::new(),
        jellyfin_server_url: "http://jellyfin.local:8096".into(),
        asset: ArtworkKind::Poster,
    }
}

#[test]
fn provider_keys_round_trip_without_printing() {
    let studio = Studio::new(
        MemoryStore::new(),
        ScriptHttp::new(Vec::new()),
        idle_process(),
    );
    assert!(
        !studio
            .provider_key_status(ImageProvider::Fal)
            .unwrap()
            .configured
    );
    let status = studio
        .save_provider_key(ImageProvider::Fal, Secret::new("  fal-key-value-1  "))
        .unwrap();
    assert!(status.configured);
    assert!(
        studio
            .provider_key_status(ImageProvider::Fal)
            .unwrap()
            .configured
    );
    let removed = studio.remove_provider_key(ImageProvider::Fal).unwrap();
    assert!(!removed.configured);
    let rendered = format!("{status:?}");
    assert!(!rendered.contains("fal-key-value-1"));
}

#[test]
fn prompt_and_higgsfield_limits_match_shipping_copy() {
    let scratch = Scratch::new();
    let studio = Studio::new(
        MemoryStore::new(),
        ScriptHttp::new(Vec::new()),
        idle_process(),
    );
    let short = runtime().block_on(studio.generate(
        &scratch.paths,
        request(ImageProvider::Fal, "too short".into()),
    ));
    assert!(matches!(short, Err(StudioError::PromptTooShort)));
    let long = "a".repeat(30_001);
    let too_long =
        runtime().block_on(studio.generate(&scratch.paths, request(ImageProvider::Fal, long)));
    assert!(matches!(too_long, Err(StudioError::PromptTooLong)));
    assert!(!ImageProvider::Higgsfield.generation_implemented());
    assert!(ImageProvider::Codex.generation_implemented());
    assert!(ImageProvider::Fal.generation_implemented());
    let saved = studio
        .save_provider_key(
            ImageProvider::Higgsfield,
            Secret::new("higgsfield-key-value"),
        )
        .unwrap();
    assert!(saved.configured);
    assert!(
        studio
            .provider_key_status(ImageProvider::Higgsfield)
            .unwrap()
            .configured
    );
    let higgsfield = runtime()
        .block_on(studio.generate(&scratch.paths, request(ImageProvider::Higgsfield, prompt())));
    let Err(error) = higgsfield else {
        panic!("higgsfield generated")
    };
    assert_eq!(
        error.to_string(),
        "Higgsfield does not currently publish an API-key image endpoint. Use its CLI/MCP account integration until developer endpoint documentation is available."
    );
}

#[test]
fn fal_download_checks_type_size_and_hides_the_key() {
    let scratch = Scratch::new();
    let secrets = MemoryStore::new();
    secrets
        .set(
            &matinee_secrets::CredentialNamespace::image_generation(),
            &matinee_secrets::CredentialKey::new("fal").unwrap(),
            &Secret::new("fal-live-key-99"),
        )
        .unwrap();
    let http = ScriptHttp::new(vec![
        Ok(StudioResponse {
            status: 200,
            headers: Vec::new(),
            body:
                br#"{"images":[{"url":"https://cdn.example/out.png","content_type":"image/png"}]}"#
                    .to_vec(),
        }),
        Ok(StudioResponse {
            status: 200,
            headers: vec![("content-type".into(), "image/png".into())],
            body: TINY_PNG.to_vec(),
        }),
    ]);
    let seen_log = http.seen();
    let studio = Studio::new(secrets, http, idle_process());
    let image = runtime()
        .block_on(studio.generate(&scratch.paths, request(ImageProvider::Fal, prompt())))
        .unwrap();
    assert!(image.local_path.contains("fal-"));
    assert!(image.data_url.starts_with("data:image/png;base64,"));
    let seen = seen_log.lock().expect("seen").clone();
    assert_eq!(seen[0].url, "https://fal.run/fal-ai/flux/dev");
    let debug = format!("{:?}", seen[0]);
    assert!(!debug.contains("fal-live-key-99"));
    assert!(
        seen[0]
            .headers
            .iter()
            .any(|(_, value)| value.contains("fal-live-key-99"))
    );

    let mismatch_secrets = MemoryStore::new();
    mismatch_secrets
        .set(
            &matinee_secrets::CredentialNamespace::image_generation(),
            &matinee_secrets::CredentialKey::new("fal").unwrap(),
            &Secret::new("fal-live-key-99"),
        )
        .unwrap();
    let mismatch = Studio::new(
        mismatch_secrets,
        ScriptHttp::new(vec![
            Ok(StudioResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"images":[{"url":"https://cdn.example/out.png","content_type":"image/jpeg"}]}"#.to_vec(),
            }),
            Ok(StudioResponse {
                status: 200,
                headers: Vec::new(),
                body: TINY_PNG.to_vec(),
            }),
        ]),
        idle_process(),
    );
    let wrong = runtime()
        .block_on(mismatch.generate(&scratch.paths, request(ImageProvider::Fal, prompt())));
    assert!(wrong.unwrap_err().to_string().contains("do not match"));

    let huge_secrets = MemoryStore::new();
    huge_secrets
        .set(
            &matinee_secrets::CredentialNamespace::image_generation(),
            &matinee_secrets::CredentialKey::new("fal").unwrap(),
            &Secret::new("fal-live-key-99"),
        )
        .unwrap();
    let huge = Studio::new(
        huge_secrets,
        ScriptHttp::new(vec![
            Ok(StudioResponse {
                status: 200,
                headers: Vec::new(),
                body: br#"{"images":[{"url":"https://cdn.example/out.png"}]}"#.to_vec(),
            }),
            Err(StudioError::ImageTooLarge),
        ]),
        idle_process(),
    );
    assert!(matches!(
        runtime().block_on(huge.generate(&scratch.paths, request(ImageProvider::Fal, prompt()))),
        Err(StudioError::ImageTooLarge)
    ));
}

#[test]
fn reference_downloads_stay_on_the_jellyfin_origin_and_honor_the_size_cap() {
    let scratch = Scratch::new();
    let secrets = MemoryStore::new();
    secrets
        .set(
            &matinee_secrets::CredentialNamespace::image_generation(),
            &matinee_secrets::CredentialKey::new("fal").unwrap(),
            &Secret::new("fal-live-key-99"),
        )
        .unwrap();
    let mut generation = request(ImageProvider::Fal, prompt());
    generation.reference_urls = vec!["http://jellyfin.local:8096/Items/1/Images/Backdrop".into()];
    let http = ScriptHttp::new(vec![
        Ok(StudioResponse {
            status: 200,
            headers: vec![("content-type".into(), "image/png".into())],
            body: TINY_PNG.to_vec(),
        }),
        Ok(StudioResponse {
            status: 200,
            headers: Vec::new(),
            body:
                br#"{"images":[{"url":"https://cdn.example/out.png","content_type":"image/png"}]}"#
                    .to_vec(),
        }),
        Ok(StudioResponse {
            status: 200,
            headers: Vec::new(),
            body: TINY_PNG.to_vec(),
        }),
    ]);
    let seen_log = http.seen();
    let studio = Studio::new(secrets, http, idle_process());
    runtime()
        .block_on(studio.generate(&scratch.paths, generation))
        .unwrap();
    let seen = seen_log.lock().expect("seen").clone();
    assert!(seen[1].url.contains("fal-ai/flux-2/edit"));

    let mut oversized = request(ImageProvider::Fal, prompt());
    oversized.reference_urls = vec!["http://other.example/secret.png".into()];
    let rejected = Studio::new(
        MemoryStore::new(),
        ScriptHttp::new(Vec::new()),
        idle_process(),
    );
    let error = runtime()
        .block_on(rejected.generate(&scratch.paths, oversized))
        .unwrap_err();
    assert!(error.to_string().contains("did not belong"));
}

#[test]
fn one_codex_job_is_active_and_timeout_is_the_shipping_sentence() {
    let scratch = Scratch::new();
    let binary = scratch.root.join("codex-bin");
    std::fs::write(&binary, b"bin").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&binary).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&binary, permissions).unwrap();
    }
    save_codex_path(&scratch.paths, &binary).unwrap();
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let saw_shell = Arc::new(Mutex::new(false));
    let process = ScriptProcess {
        result: Mutex::new(Some(Err(ProcessError::Timeout))),
        started: Arc::clone(&started),
        release: Arc::clone(&release),
        calls: Arc::clone(&calls),
        saw_shell: Arc::clone(&saw_shell),
    };
    let studio = Arc::new(Studio::new(
        MemoryStore::new(),
        ScriptHttp::new(Vec::new()),
        process,
    ));
    let runtime = runtime();
    runtime.block_on(async {
        let first = {
            let studio = Arc::clone(&studio);
            let paths = scratch.paths.clone();
            tokio::spawn(async move { studio.generate(&paths, request(ImageProvider::Codex, prompt())).await })
        };
        started.notified().await;
        let second = studio
            .generate(&scratch.paths, request(ImageProvider::Codex, prompt()))
            .await;
        assert!(matches!(second, Err(StudioError::GenerationBusy)));
        release.notify_one();
        let finished = first.await.unwrap();
        assert!(matches!(finished, Err(StudioError::GenerationTimeout)));
        assert_eq!(
            finished.unwrap_err().to_string(),
            "Codex image generation timed out after 12 minutes. The request was stopped cleanly; retry it or choose fal.ai in Settings."
        );
    });
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(!*saw_shell.lock().unwrap());
}

#[test]
fn assignment_and_export_stay_inside_trusted_roots() {
    let scratch = Scratch::new();
    let studio = Studio::new(
        MemoryStore::new(),
        ScriptHttp::new(Vec::new()),
        idle_process(),
    );
    let generated = scratch.paths.app_file("generated-posters");
    std::fs::create_dir_all(&generated).unwrap();
    let source = generated.join("fal-1.png");
    std::fs::write(&source, TINY_PNG).unwrap();
    assert!(
        studio
            .assign_generated_poster(&scratch.paths, "../escape", source.to_str().unwrap())
            .is_err()
    );
    let poster = studio
        .assign_generated_poster(&scratch.paths, "item_1", source.to_str().unwrap())
        .unwrap();
    assert!(poster.local_path.contains("custom-posters"));
    assert!(poster.local_path.ends_with("item_1.png"));
    let listed = studio.list_custom_posters(&scratch.paths).unwrap();
    assert_eq!(listed.len(), 1);

    let outside = scratch.root.join("outside.png");
    std::fs::write(&outside, TINY_PNG).unwrap();
    assert!(
        studio
            .export_generated_image(&scratch.paths, outside.to_str().unwrap(), "Title")
            .unwrap_err()
            .to_string()
            .contains("artwork it generated")
    );
    let exported = studio
        .export_generated_image(&scratch.paths, &poster.local_path, "../../Secrets")
        .unwrap();
    assert!(exported.contains("pictures"));
    assert!(!exported.contains(".."));

    let movie = scratch.paths.home().join("media/movies/Film.mkv");
    std::fs::write(&movie, b"video").unwrap();
    let first = studio.export_poster_to_media_folder(
        &scratch.paths,
        &poster.local_path,
        movie.to_str().unwrap(),
        false,
    );
    assert!(first.unwrap().ends_with("poster.jpg"));
    let again = studio.export_poster_to_media_folder(
        &scratch.paths,
        &poster.local_path,
        movie.to_str().unwrap(),
        false,
    );
    assert_eq!(again.unwrap_err().to_string(), "POSTER_EXISTS");
    studio
        .export_poster_to_media_folder(
            &scratch.paths,
            &poster.local_path,
            movie.to_str().unwrap(),
            true,
        )
        .unwrap();
}

#[test]
fn oversized_manifests_are_rejected_before_parsing() {
    let scratch = Scratch::new();
    let movie = scratch.paths.home().join("media/movies/Film.mkv");
    std::fs::write(&movie, b"video").unwrap();
    let manifest = movie.parent().unwrap().join("movie.mf.json");
    std::fs::write(&manifest, vec![b' '; (2 * 1024 * 1024) + 8]).unwrap();
    let studio = Studio::new(
        MemoryStore::new(),
        ScriptHttp::new(Vec::new()),
        idle_process(),
    );
    let error = studio
        .load_manifest(&scratch.paths, movie.to_str().unwrap())
        .unwrap_err();
    assert!(error.to_string().contains("2 MB"));
    assert!(!error.to_string().contains("schema"));
    std::fs::write(&manifest, b"{").unwrap();
    let malformed = studio
        .load_manifest(&scratch.paths, movie.to_str().unwrap())
        .unwrap_err();
    assert!(malformed.to_string().contains("schema"));
}

#[test]
fn provider_redirects_are_checked_on_every_hop() {
    let scratch = Scratch::new();
    let secrets = MemoryStore::new();
    secrets
        .set(
            &matinee_secrets::CredentialNamespace::image_generation(),
            &matinee_secrets::CredentialKey::new("fal").unwrap(),
            &Secret::new("fal-live-key-99"),
        )
        .unwrap();
    let http = ScriptHttp::new(vec![
        Ok(fal_json(
            "https://v3.fal.media/files/a.png?token=signed-secret",
        )),
        Ok(redirect("/files/b.png")),
        Ok(png_response()),
    ]);
    let seen = http.seen();
    let studio = Studio::new(secrets, http, idle_process());
    let image = runtime()
        .block_on(studio.generate(&scratch.paths, request(ImageProvider::Fal, prompt())))
        .unwrap();
    assert!(image.local_path.contains("fal-"));
    let seen = seen.lock().expect("seen").clone();
    assert_eq!(
        seen[1].url,
        "https://v3.fal.media/files/a.png?token=signed-secret"
    );
    assert_eq!(seen[2].url, "https://v3.fal.media/files/b.png");
    assert!(!format!("{:?}", seen[1]).contains("signed-secret"));

    let refused = [
        "http://127.0.0.1/poster.png",
        "http://10.1.2.3/poster.png",
        "http://[::1]/poster.png",
        "http://169.254.169.254/latest",
        "file:///etc/passwd",
        "ftp://cdn.example/poster.png",
        "https://user:secret@cdn.example/a.png",
    ];
    for url in refused {
        let error = generate_from(url, vec![]);
        assert!(error.is_err(), "{url}");
    }
    assert!(
        generate_from(
            "https://v3.fal.media/files/a.png",
            vec![redirect("http://cdn.example/poster.png")]
        )
        .is_err()
    );
    assert!(generate_from("https://v3.fal.media/files/a.png", vec![redirect("")]).is_err());
    assert!(
        generate_from(
            "https://v3.fal.media/files/a.png",
            vec![redirect("http://[")]
        )
        .is_err()
    );
    assert!(
        generate_from(
            "https://v3.fal.media/files/a.png",
            vec![
                redirect("https://v3.fal.media/files/a.png"),
                redirect("https://v3.fal.media/files/a.png"),
                redirect("https://v3.fal.media/files/a.png"),
            ]
        )
        .unwrap_err()
        .to_string()
        .contains("too many times")
    );
}

fn generate_from(
    url: &str,
    hops: Vec<StudioResponse>,
) -> Result<crate::model::GeneratedImage, StudioError> {
    let scratch = Scratch::new();
    let secrets = MemoryStore::new();
    secrets
        .set(
            &matinee_secrets::CredentialNamespace::image_generation(),
            &matinee_secrets::CredentialKey::new("fal").unwrap(),
            &Secret::new("fal-live-key-99"),
        )
        .unwrap();
    let mut responses = vec![Ok(fal_json(url))];
    responses.extend(hops.into_iter().map(Ok));
    let studio = Studio::new(secrets, ScriptHttp::new(responses), idle_process());
    runtime().block_on(studio.generate(&scratch.paths, request(ImageProvider::Fal, prompt())))
}

fn fal_json(url: &str) -> StudioResponse {
    StudioResponse {
        status: 200,
        headers: Vec::new(),
        body: format!(r#"{{"images":[{{"url":"{url}","content_type":"image/png"}}]}}"#)
            .into_bytes(),
    }
}

fn redirect(location: &str) -> StudioResponse {
    StudioResponse {
        status: 302,
        headers: vec![("location".into(), location.into())],
        body: Vec::new(),
    }
}

fn png_response() -> StudioResponse {
    StudioResponse {
        status: 200,
        headers: vec![("content-type".into(), "image/png".into())],
        body: TINY_PNG.to_vec(),
    }
}

fn idle_process() -> ScriptProcess {
    ScriptProcess {
        result: Mutex::new(None),
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
        calls: Arc::new(AtomicUsize::new(0)),
        saw_shell: Arc::new(Mutex::new(false)),
    }
}
