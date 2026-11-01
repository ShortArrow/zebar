/// Logs every panic as an error, so that a panic in a background task
/// reaches `errors.log` instead of only stderr.
///
/// The hook that was installed before still runs after the log.
pub fn install_panic_hook() {
  let previous_hook = std::panic::take_hook();

  std::panic::set_hook(Box::new(move |info| {
    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("<unnamed>");
    let backtrace = std::backtrace::Backtrace::force_capture();
    tracing::error!("Thread '{thread_name}' {info}\n{backtrace}");

    previous_hook(info);
  }));
}

#[cfg(test)]
mod tests {
  use std::{
    io::Write,
    panic,
    sync::{Arc, Mutex, Once},
  };

  use tracing_subscriber::fmt::MakeWriter;

  use super::install_panic_hook;

  #[derive(Clone, Default)]
  struct CapturedLog(Arc<Mutex<Vec<u8>>>);

  impl CapturedLog {
    fn text(&self) -> String {
      String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
  }

  impl Write for CapturedLog {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
      self.0.lock().unwrap().extend_from_slice(buf);
      Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
      Ok(())
    }
  }

  impl<'a> MakeWriter<'a> for CapturedLog {
    type Writer = CapturedLog;

    fn make_writer(&'a self) -> Self::Writer {
      self.clone()
    }
  }

  fn install_once() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(install_panic_hook);
  }

  fn capture_errors(run: impl FnOnce()) -> String {
    install_once();
    let log = CapturedLog::default();
    let subscriber = tracing_subscriber::fmt()
      .with_max_level(tracing::Level::ERROR)
      .with_ansi(false)
      .with_writer(log.clone())
      .finish();
    tracing::subscriber::with_default(subscriber, run);
    log.text()
  }

  #[test]
  fn panic_is_logged_with_message_and_location() {
    let log = capture_errors(|| {
      let _ = panic::catch_unwind(|| panic!("boom from a test"));
    });

    assert!(log.contains("boom from a test"), "log was: {log}");
    assert!(log.contains("panic_log.rs:"), "log was: {log}");
  }

  #[test]
  fn panic_in_spawned_task_is_logged() {
    let log = capture_errors(|| {
      let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
      let result = runtime.block_on(async {
        tokio::spawn(async { panic!("boom from a task") }).await
      });
      assert!(result.unwrap_err().is_panic());
    });

    assert!(log.contains("boom from a task"), "log was: {log}");
  }
}
