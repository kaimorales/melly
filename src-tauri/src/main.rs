#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod inline_secret;
mod login_state;
mod secure_input;
use melly_core::{
    auth,
    auth_namespace::AuthNamespaces,
    credentials::{KeychainStore, SecretStore},
    engine::{Engine, SEARCH_KEY_ID},
    models::*,
    providers,
    storage::Store,
};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};
use tokio_util::sync::CancellationToken;
struct AppState {
    engine: Arc<Engine>,
    login: Mutex<login_state::LoginState>,
    plan_namespaces: Mutex<AuthNamespaces>,
    started: std::time::Instant,
    startup_recorded: std::sync::atomic::AtomicBool,
    shutdown_requested: std::sync::atomic::AtomicBool,
    shutdown_complete: std::sync::atomic::AtomicBool,
}
type Reply<T> = Result<T, String>;
#[tauri::command]
async fn frontend_ready(app: tauri::AppHandle, s: State<'_, AppState>) -> Reply<()> {
    if !s
        .startup_recorded
        .swap(true, std::sync::atomic::Ordering::Relaxed)
    {
        let elapsed = s.started.elapsed().as_millis();
        eprintln!(
            "Melly startup: {} ms from process entry to rendered UI",
            elapsed
        );
        if let Ok(dir) = app.path().app_data_dir() {
            let metric = serde_json::json!({"processToRenderedUiMs":elapsed,"debugBuild":cfg!(debug_assertions)});
            let _ = std::fs::write(dir.join("startup-metric.json"), metric.to_string());
        }
    }
    Ok(())
}
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[tauri::command]
async fn bootstrap(s: State<'_, AppState>) -> Reply<Bootstrap> {
    let mut bootstrap = s.engine.bootstrap().map_err(error)?;
    let login = s.login.lock().unwrap();
    if let Some(provider) = bootstrap
        .providers
        .iter_mut()
        .find(|p| p.spec.id == "chatgpt")
    {
        if login.is_pending() {
            provider.status = "Sign-in pending".into();
        } else if let Some(reason) = s.plan_namespaces.lock().unwrap().startup_error() {
            provider.configured = false;
            provider.status = reason.into();
        }
    }
    Ok(bootstrap)
}
#[tauri::command]
async fn create_chat(s: State<'_, AppState>, folder_id: Option<String>) -> Reply<Chat> {
    s.engine
        .store
        .lock()
        .unwrap()
        .create_chat(folder_id.as_deref())
        .map_err(error)
}
#[tauri::command]
async fn chat(s: State<'_, AppState>, chat_id: String) -> Reply<Chat> {
    s.engine.store.lock().unwrap().chat(&chat_id).map_err(error)
}
#[tauri::command]
async fn rename_chat(s: State<'_, AppState>, chat_id: String, title: String) -> Reply<()> {
    s.engine
        .store
        .lock()
        .unwrap()
        .rename_chat(&chat_id, &title)
        .map_err(error)
}
#[tauri::command]
async fn delete_chat(s: State<'_, AppState>, chat_id: String) -> Reply<()> {
    s.engine.delete_chat(&chat_id).map_err(error)
}
#[tauri::command]
async fn move_chat(
    s: State<'_, AppState>,
    chat_id: String,
    folder_id: Option<String>,
) -> Reply<()> {
    s.engine
        .move_chat(&chat_id, folder_id.as_deref())
        .map_err(error)
}
#[tauri::command]
async fn create_folder(s: State<'_, AppState>, title: String) -> Reply<Folder> {
    s.engine
        .store
        .lock()
        .unwrap()
        .create_folder(&title)
        .map_err(error)
}
#[tauri::command]
async fn rename_folder(s: State<'_, AppState>, folder_id: String, title: String) -> Reply<()> {
    s.engine
        .store
        .lock()
        .unwrap()
        .rename_folder(&folder_id, &title)
        .map_err(error)
}
#[tauri::command]
async fn messages(
    s: State<'_, AppState>,
    chat_id: String,
    before: Option<i64>,
) -> Reply<Vec<Message>> {
    s.engine
        .store
        .lock()
        .unwrap()
        .messages(&chat_id, before)
        .map_err(error)
}
#[tauri::command]
async fn search_chats(s: State<'_, AppState>, query: String) -> Reply<Vec<Chat>> {
    s.engine
        .store
        .lock()
        .unwrap()
        .search_chats(&query)
        .map_err(error)
}
#[tauri::command]
async fn save_settings(s: State<'_, AppState>, settings: Settings) -> Reply<()> {
    if !providers::catalog()
        .iter()
        .any(|p| p.id == settings.default_provider && p.disabled_reason.is_none())
    {
        return Err("Choose an available default provider".into());
    }
    s.engine
        .store
        .lock()
        .unwrap()
        .save_settings(&settings)
        .map_err(error)
}
#[tauri::command]
async fn save_connection(s: State<'_, AppState>, connection: Connection) -> Reply<()> {
    providers::validate_connection(&connection).map_err(error)?;
    s.engine
        .store
        .lock()
        .unwrap()
        .save_connection(&connection)
        .map_err(error)
}
#[cfg(target_os = "macos")]
#[tauri::command]
async fn open_provider_key_field(
    window: tauri::WebviewWindow,
    generation: u64,
    provider_id: String,
    rect: inline_secret::FieldRect,
    theme: String,
    configured: bool,
) -> Reply<()> {
    if generation == 0 || !matches!(theme.as_str(), "light" | "dark") {
        return Err("Invalid API key field".into());
    }
    let provider = providers::catalog()
        .into_iter()
        .find(|provider| provider.id == provider_id)
        .ok_or("Unknown provider")?;
    if matches!(provider.id.as_str(), "mock" | "chatgpt" | "muse")
        || provider.disabled_reason.is_some()
    {
        return Err("This connection does not accept an API key".into());
    }
    inline_secret::open(window, generation, provider_id, rect, theme, configured).await
}
#[cfg(target_os = "macos")]
#[tauri::command]
async fn update_provider_key_field(
    window: tauri::WebviewWindow,
    generation: u64,
    rect: inline_secret::FieldRect,
    theme: String,
) -> Reply<()> {
    if !matches!(theme.as_str(), "light" | "dark") {
        return Err("Invalid appearance".into());
    }
    inline_secret::update(window, generation, rect, theme).await
}
#[cfg(target_os = "macos")]
#[tauri::command]
async fn focus_provider_key_field(window: tauri::WebviewWindow, generation: u64) -> Reply<()> {
    inline_secret::focus(window, generation).await
}
#[cfg(target_os = "macos")]
#[tauri::command]
async fn save_provider_key_field(
    window: tauri::WebviewWindow,
    s: State<'_, AppState>,
    generation: u64,
    provider_id: String,
) -> Reply<bool> {
    let connection = s.engine.connection(&provider_id).map_err(error)?;
    providers::validate_connection(&connection).map_err(error)?;
    let credential_id = providers::credential_id(&connection);
    inline_secret::save(
        window,
        generation,
        provider_id,
        credential_id,
        s.engine.vault.clone(),
    )
    .await
}
#[cfg(target_os = "macos")]
#[tauri::command]
async fn close_provider_key_field(window: tauri::WebviewWindow, generation: u64) -> Reply<()> {
    inline_secret::close(window, generation).await
}
#[tauri::command]
async fn remove_provider_key(s: State<'_, AppState>, provider_id: String) -> Reply<()> {
    let c = s.engine.connection(&provider_id).map_err(error)?;
    s.engine
        .vault
        .delete(&providers::credential_id(&c))
        .map_err(error)
}
#[tauri::command]
async fn enter_search_key(app: tauri::AppHandle, s: State<'_, AppState>) -> Reply<bool> {
    if let Some(key) =
        secure_input::prompt_secret(app, "Tavily search · separate billing".into()).await?
    {
        s.engine.vault.set(SEARCH_KEY_ID, &key).map_err(error)?;
        Ok(true)
    } else {
        Ok(false)
    }
}
#[tauri::command]
async fn remove_search_key(s: State<'_, AppState>) -> Reply<()> {
    s.engine.vault.delete(SEARCH_KEY_ID).map_err(error)
}
#[tauri::command]
async fn connect_chatgpt(app: tauri::AppHandle, s: State<'_, AppState>) -> Reply<()> {
    let (generation, cancel, attempt) = {
        let mut login = s.login.lock().unwrap();
        if s.shutdown_requested
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err("Melly is closing".into());
        }
        let (generation, cancel) = login.begin()?;
        let attempt = match s.plan_namespaces.lock().unwrap().begin() {
            Ok(attempt) => attempt,
            Err(error) => {
                cancel.cancel();
                return Err(error.to_string());
            }
        };
        (generation, cancel, attempt)
    };
    let failure = Arc::new(Mutex::new(None::<String>));
    let callback_failure = failure.clone();
    let callback_cancel = cancel.clone();
    let on_url = Arc::new(move |url: String| {
        if let Err(reason) = login_state::open_authorization(&url, &callback_cancel, |url| {
            open::that(url).map_err(|_| ())
        }) {
            *callback_failure.lock().unwrap() = Some(reason.into());
        }
    });
    tauri::async_runtime::spawn(async move {
        if !login_state::publish_if_current(&app.state::<AppState>().login, generation, || {
            let _ = app.emit(
                "connection-update",
                serde_json::json!({"status":"signing-in","message":"Finish ChatGPT sign-in and plan authorization in your browser"}),
            );
        }) {
            return;
        }
        let result = attempt.sign_in(on_url, cancel.clone()).await;
        cancel.cancel();
        let result = if let Some(reason) = failure.lock().unwrap().take() {
            Err(anyhow::anyhow!(reason))
        } else {
            result
        };
        let state = app.state::<AppState>();
        login_state::publish_if_current(&state.login, generation, || {
            let result = result.and_then(|authorized| {
                let vault = state.plan_namespaces.lock().unwrap().activate(authorized)?;
                state.engine.select_plan_vault(vault);
                Ok(())
            });
            let status = match result {
                Ok(()) => {
                    serde_json::json!({"status":"connected","message":"ChatGPT plan authorized. Live chat can now be tested."})
                }
                Err(error) => serde_json::json!({"status":"error","message":error.to_string()}),
            };
            let _ = app.emit("connection-update", status);
        });
    });
    Ok(())
}
#[tauri::command]
async fn disconnect_chatgpt(s: State<'_, AppState>) -> Reply<()> {
    let mut login = s.login.lock().unwrap();
    if s.shutdown_requested
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err("Melly is closing".into());
    }
    login.cancel();
    let mut namespaces = s.plan_namespaces.lock().unwrap();
    let result = namespaces.detach();
    // Detach in this process even if public metadata could not be persisted.
    // Old protected credentials are never opened or deleted by this command.
    s.engine.select_plan_vault(namespaces.selected_vault());
    result.map_err(error)
}
#[tauri::command]
async fn models(s: State<'_, AppState>, provider_id: String) -> Reply<Vec<ModelInfo>> {
    let c = s.engine.connection(&provider_id).map_err(error)?;
    let key = s.engine.credential(&c).await.map_err(error)?;
    providers::list_models(&c, key.as_deref(), CancellationToken::new())
        .await
        .map_err(error)
}
#[tauri::command]
async fn start_chat(
    app: tauri::AppHandle,
    s: State<'_, AppState>,
    input: SendInput,
) -> Reply<Message> {
    s.engine
        .start(
            input,
            Arc::new(move |update| {
                let _ = app.emit("chat-stream", update);
            }),
        )
        .map_err(error)
}
#[tauri::command]
async fn stop_chat(s: State<'_, AppState>, chat_id: String) -> Reply<()> {
    s.engine.stop(&chat_id);
    Ok(())
}
#[tauri::command]
async fn demo_seed(s: State<'_, AppState>) -> Reply<Bootstrap> {
    s.engine.store.lock().unwrap().seed_demo().map_err(error)?;
    s.engine.bootstrap().map_err(error)
}
#[tauri::command]
async fn open_source(source: Source) -> Reply<()> {
    if source.kind != "web" {
        return Err("Open chat sources inside Melly".into());
    }
    let u = providers::public_url(source.url.as_deref().ok_or("Source URL is missing")?)
        .map_err(error)?;
    open::that(u.as_str()).map_err(|_| "Could not open the source in your browser".into())
}
fn main() {
    let started = std::time::Instant::now();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .setup(move |app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
            }
            let store = Arc::new(Mutex::new(Store::open(&dir.join("melly.sqlite3"))?));
            let vault: Arc<dyn SecretStore> = Arc::new(KeychainStore::new());
            let engine = Arc::new(Engine::new(store, vault.clone()));
            let namespaces = AuthNamespaces::open(vault, dir.join("chatgpt-namespaces.json"))?;
            engine.select_plan_vault(namespaces.selected_vault());
            app.manage(AppState {
                engine,
                login: Mutex::new(login_state::LoginState::default()),
                plan_namespaces: Mutex::new(namespaces),
                started,
                startup_recorded: std::sync::atomic::AtomicBool::new(false),
                shutdown_requested: std::sync::atomic::AtomicBool::new(false),
                shutdown_complete: std::sync::atomic::AtomicBool::new(false),
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                window.state::<AppState>().engine.stop_all();
            }
        })
        .invoke_handler(tauri::generate_handler![
            frontend_ready,
            bootstrap,
            create_chat,
            rename_chat,
            delete_chat,
            move_chat,
            create_folder,
            rename_folder,
            messages,
            chat,
            search_chats,
            save_settings,
            save_connection,
            open_provider_key_field,
            update_provider_key_field,
            focus_provider_key_field,
            save_provider_key_field,
            close_provider_key_field,
            remove_provider_key,
            enter_search_key,
            remove_search_key,
            connect_chatgpt,
            disconnect_chatgpt,
            models,
            start_chat,
            stop_chat,
            demo_seed,
            open_source
        ]);
    let app = builder
        .build(tauri::generate_context!())
        .expect("Melly could not start");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            let state = app.state::<AppState>();
            if state
                .shutdown_complete
                .load(std::sync::atomic::Ordering::Acquire)
            {
                return;
            }
            api.prevent_exit();
            if state
                .shutdown_requested
                .swap(true, std::sync::atomic::Ordering::AcqRel)
            {
                return; // Repeated Quit must not bypass the current drain.
            }
            auth::begin_shutdown();
            state.login.lock().unwrap().cancel();
            state.engine.stop_all();
            let engine = state.engine.clone();
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let persist_chat = async {
                    for _ in 0..100 {
                        if engine.active_count() == 0 {
                            break;
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    }
                };
                // Stop can remove the chat before a detached rotating renewal
                // finishes. Its lifetime is tracked independently of the chat.
                let _ = tokio::join!(persist_chat, auth::drain_pending(auth::EXIT_DRAIN_TIMEOUT));
                app.state::<AppState>()
                    .shutdown_complete
                    .store(true, std::sync::atomic::Ordering::Release);
                app.exit(0);
            });
        }
    });
}
