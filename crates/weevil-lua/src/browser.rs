use std::sync::Arc;

use chromiumoxide::Page;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::handler::Handler;
use futures_util::StreamExt;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use url::Url;

use crate::error::LuaPluginError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserLaunchOptions {
    pub headless: bool,
    pub executable_path: Option<String>,
    pub no_sandbox: bool,
    pub args: Vec<String>,
}

impl Default for BrowserLaunchOptions {
    fn default() -> Self {
        Self {
            headless: true,
            executable_path: None,
            no_sandbox: false,
            args: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct BrowserSession {
    browser: Arc<Mutex<Browser>>,
    handler_task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

#[derive(Clone)]
pub struct BrowserPage {
    page: Page,
}

impl BrowserSession {
    pub async fn launch(options: BrowserLaunchOptions) -> Result<Self, LuaPluginError> {
        let config = browser_config(options)?;
        let (browser, handler) = Browser::launch(config)
            .await
            .map_err(|err| browser_operation_error("launching browser", err))?;
        Ok(Self::new(browser, handler))
    }

    pub async fn connect(endpoint: &str) -> Result<Self, LuaPluginError> {
        validate_endpoint(endpoint)?;
        let (browser, handler) = Browser::connect(endpoint.to_string())
            .await
            .map_err(|err| browser_operation_error("connecting browser", err))?;
        Ok(Self::new(browser, handler))
    }

    pub async fn new_page(&self, url: Option<&str>) -> Result<BrowserPage, LuaPluginError> {
        let browser = self.browser.lock().await;
        let page = browser
            .new_page(url.unwrap_or("about:blank"))
            .await
            .map_err(|err| browser_operation_error("creating page", err))?;
        Ok(BrowserPage { page })
    }

    pub async fn websocket_address(&self) -> String {
        let browser = self.browser.lock().await;
        browser.websocket_address().clone()
    }

    pub async fn close(&self) -> Result<(), LuaPluginError> {
        {
            let mut browser = self.browser.lock().await;
            browser
                .close()
                .await
                .map_err(|err| browser_operation_error("closing browser", err))?;
            browser
                .wait()
                .await
                .map_err(|err| browser_operation_error("waiting browser exit", err))?;
        }
        if let Some(task) = self.handler_task.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
        Ok(())
    }

    fn new(browser: Browser, handler: Handler) -> Self {
        Self {
            browser: Arc::new(Mutex::new(browser)),
            handler_task: Arc::new(Mutex::new(Some(spawn_handler_task(handler)))),
        }
    }
}

impl Drop for BrowserSession {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.handler_task.try_lock() {
            if let Some(task) = guard.take() {
                task.abort();
            }
        }
    }
}

impl BrowserPage {
    pub async fn goto(&self, url: &str) -> Result<(), LuaPluginError> {
        self.page
            .goto(url)
            .await
            .map_err(|err| browser_operation_error("navigating page", err))?;
        Ok(())
    }

    pub async fn content(&self) -> Result<String, LuaPluginError> {
        self.page
            .content()
            .await
            .map_err(|err| browser_operation_error("reading page content", err))
    }

    pub async fn url(&self) -> Result<Option<String>, LuaPluginError> {
        self.page
            .url()
            .await
            .map_err(|err| browser_operation_error("reading page url", err))
    }

    pub async fn title(&self) -> Result<String, LuaPluginError> {
        let eval = self
            .page
            .evaluate("document.title")
            .await
            .map_err(|err| browser_operation_error("evaluating document.title", err))?;
        eval.into_value::<String>()
            .map_err(|err| browser_operation_error("decoding page title", err))
    }

    pub async fn click(&self, selector: &str) -> Result<(), LuaPluginError> {
        self.page
            .find_element(selector)
            .await
            .map_err(|err| browser_operation_error("finding element for click", err))?
            .click()
            .await
            .map_err(|err| browser_operation_error("clicking element", err))?;
        Ok(())
    }

    pub async fn type_text(&self, selector: &str, text: &str) -> Result<(), LuaPluginError> {
        self.page
            .find_element(selector)
            .await
            .map_err(|err| browser_operation_error("finding element for typing", err))?
            .click()
            .await
            .map_err(|err| browser_operation_error("focusing element", err))?
            .type_str(text)
            .await
            .map_err(|err| browser_operation_error("typing text", err))?;
        Ok(())
    }

    pub async fn press_key(&self, selector: &str, key: &str) -> Result<(), LuaPluginError> {
        self.page
            .find_element(selector)
            .await
            .map_err(|err| browser_operation_error("finding element for key press", err))?
            .press_key(key)
            .await
            .map_err(|err| browser_operation_error("pressing key", err))?;
        Ok(())
    }

    pub async fn set_user_agent(&self, user_agent: &str) -> Result<(), LuaPluginError> {
        self.page
            .set_user_agent(user_agent)
            .await
            .map_err(|err| browser_operation_error("setting user agent", err))?;
        Ok(())
    }

    pub async fn reload(&self) -> Result<(), LuaPluginError> {
        self.page
            .reload()
            .await
            .map_err(|err| browser_operation_error("reloading page", err))?;
        Ok(())
    }

    pub async fn wait_for_navigation(&self) -> Result<(), LuaPluginError> {
        self.page
            .wait_for_navigation()
            .await
            .map_err(|err| browser_operation_error("waiting for navigation", err))?;
        Ok(())
    }

    pub async fn close(&self) -> Result<(), LuaPluginError> {
        self.page
            .clone()
            .close()
            .await
            .map_err(|err| browser_operation_error("closing page", err))
    }
}

fn browser_config(options: BrowserLaunchOptions) -> Result<BrowserConfig, LuaPluginError> {
    let mut builder = BrowserConfig::builder();
    if !options.headless {
        builder = builder.with_head();
    }
    if options.no_sandbox {
        builder = builder.no_sandbox();
    }
    if let Some(path) = options.executable_path {
        builder = builder.chrome_executable(path);
    }
    if !options.args.is_empty() {
        builder = builder.args(options.args);
    }
    builder
        .build()
        .map_err(|err| browser_operation_error("building browser config", err))
}

fn validate_endpoint(endpoint: &str) -> Result<(), LuaPluginError> {
    let parsed = Url::parse(endpoint).map_err(|_| LuaPluginError::BrowserEndpointInvalid {
        value: endpoint.to_string(),
    })?;
    match parsed.scheme() {
        "ws" | "wss" | "http" | "https" => Ok(()),
        _ => Err(LuaPluginError::BrowserEndpointInvalid {
            value: endpoint.to_string(),
        }),
    }
}

fn spawn_handler_task(mut handler: Handler) -> JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    })
}

fn browser_operation_error(context: &str, error: impl std::fmt::Display) -> LuaPluginError {
    LuaPluginError::BrowserOperation {
        context: context.to_string(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::validate_endpoint;

    #[test]
    fn accepts_ws_and_http_endpoints() {
        assert!(validate_endpoint("ws://127.0.0.1:9222/devtools/browser/x").is_ok());
        assert!(validate_endpoint("wss://example.invalid/devtools/browser/x").is_ok());
        assert!(validate_endpoint("http://127.0.0.1:9222").is_ok());
        assert!(validate_endpoint("https://browser.invalid").is_ok());
    }

    #[test]
    fn rejects_non_browser_endpoint_schemes() {
        let err = validate_endpoint("file:///tmp/browser")
            .err()
            .expect("invalid endpoint should fail");
        assert!(err.to_string().contains("browser endpoint"));
    }
}
