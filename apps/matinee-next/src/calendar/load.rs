//! The Calendar's calls to Radarr and Sonarr. This is the only file that makes
//! them. The screen hands the model's requests here, and each one runs on the
//! service runtime and answers through a receiver.
//!
//! Each call takes the provider and the window it was asked for. A connection
//! check reads the vault on a blocking thread, since the keyring can block.
//! Nothing here logs: the service's errors are already typed and redacted, and
//! the screen shows only their kind.

use std::sync::Arc;

use chrono::{SecondsFormat, Utc};
use matinee_integrations::{Integrations, ReqwestTransport};

use super::model::{Request, Response, SourceFailure};
use crate::runtime::ServiceRuntime;
use crate::store::SharedStore;

/// The one integrations service the native app holds. Its vault is the shared
/// store, so the namespace is the shipping app's.
pub(crate) type CalendarService = Integrations<SharedStore, ReqwestTransport>;

/// Start `request` on the service runtime. The task and the receiver are the
/// caller's to hold: dropping or aborting the task ends the call.
pub(crate) fn spawn(
    runtime: &ServiceRuntime,
    service: &Arc<CalendarService>,
    request: Request,
) -> (
    tokio::task::JoinHandle<()>,
    tokio::sync::oneshot::Receiver<Response>,
) {
    let service = Arc::clone(service);
    runtime.spawn(async move { answer(service, request).await })
}

async fn answer(service: Arc<CalendarService>, request: Request) -> Response {
    match request {
        Request::Link { ticket, provider } => {
            let checked = tokio::task::spawn_blocking(move || service.key_status(provider)).await;
            let result = match checked {
                Ok(Ok(status)) => Ok(status.configured),
                Ok(Err(error)) => Err(SourceFailure::from_error(&error)),
                Err(_) => Err(SourceFailure::Unavailable),
            };
            Response::Link {
                ticket,
                provider,
                result,
            }
        }
        Request::Releases {
            ticket,
            provider,
            window,
        } => {
            let (from, to) = window.fetch_range();
            // Second precision with a `Z`, as the shipping app sends.
            let start = from.to_rfc3339_opts(SecondsFormat::Secs, true);
            let end = to.to_rfc3339_opts(SecondsFormat::Secs, true);
            let result = service
                .fetch_calendar(provider, &start, &end)
                .await
                .map_err(|error| SourceFailure::from_error(&error));
            Response::Releases {
                ticket,
                provider,
                at: Utc::now(),
                result,
            }
        }
    }
}
