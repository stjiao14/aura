pub mod models;
pub mod recorder;
pub mod session;
pub mod system;

/// Safely acquire a `Mutex` lock, returning an error string instead of panicking
/// on a poisoned lock.
pub(crate) fn safe_lock<T>(
    mutex: &std::sync::Mutex<T>,
) -> Result<std::sync::MutexGuard<'_, T>, String> {
    mutex.lock().map_err(|e| format!("Lock poisoned: {e}"))
}

/// Extension trait to convert any `Result<T, E: Display>` into `Result<T, String>`
/// in a single `.friendly()?` call, replacing repetitive `.map_err(|e| e.to_string())?`.
pub(crate) trait FriendlyError<T> {
    fn friendly(self) -> Result<T, String>;
}

impl<T, E: std::fmt::Display> FriendlyError<T> for Result<T, E> {
    fn friendly(self) -> Result<T, String> {
        self.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::recorder::friendly_error;
    use super::FriendlyError;

    #[test]
    fn friendly_error_maps_model_path_error() {
        let msg = friendly_error("model_path is missing");
        assert!(msg.contains("Settings"), "should direct user to settings");
    }

    #[test]
    fn friendly_error_maps_api_key_error() {
        let msg = friendly_error("api_key invalid, got 401 Unauthorized");
        assert!(msg.to_lowercase().contains("api key") || msg.contains("API key"));
    }

    #[test]
    fn friendly_error_maps_connection_refused() {
        let msg = friendly_error("Connection refused (os error 111)");
        assert!(msg.contains("Ollama") || msg.contains("provider"));
    }

    #[test]
    fn friendly_error_preserves_unknown_errors() {
        let msg = friendly_error("some completely unknown failure");
        assert!(msg.contains("some completely unknown failure"));
    }

    #[test]
    fn friendly_trait_converts_errors_to_string() {
        let err: Result<(), std::io::Error> =
            Err(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"));
        let result: Result<(), String> = err.friendly();
        assert_eq!(result.unwrap_err(), "missing");
    }

    #[test]
    fn friendly_trait_preserves_ok() {
        let ok: Result<i32, std::io::Error> = Ok(42);
        assert_eq!(ok.friendly().unwrap(), 42);
    }
}
