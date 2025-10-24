//! # yaaxum-error
//! Yet Another Axum Error Handler
//!
//! This crate uses `eyre` to capture the error,
//! the error is then returned to the browser or
//! whatever it is, it's then nicely formatted to
//! a webpage using `ansi_to_html`
use std::fmt::{Debug, Display};

use axum::{
    body::Body,
    http::StatusCode,
    response::{Html, IntoResponse},
};
use color_eyre::eyre::eyre;
use tower_http::catch_panic::ResponseForPanic;

pub type Result<T> = std::result::Result<T, Error>;

pub struct Error(pub StatusCode, pub color_eyre::eyre::Report);

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ansi_string = format!("{:?}", self);
        let error = ansi_to_html::convert(&ansi_string).unwrap();
        write!(
            f,
            "<!DOCTYPE html><html><head><meta charset=\"utf8\"></head><body><pre><code>{}</code></pre></body></html>",
            error
        )
    }
}

impl Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.1.handler().debug(self.1.as_ref(), f)
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        (self.0, Html(format!("{}", self))).into_response()
    }
}

impl From<Error> for Box<dyn std::error::Error + Sync + Send> {
    fn from(value: Error) -> Self {
        value.1.into()
    }
}

#[derive(Clone, Copy)]
pub struct PanicHandler;

impl ResponseForPanic for PanicHandler {
    type ResponseBody = Body;

    fn response_for_panic(
        &mut self,
        err: Box<dyn std::any::Any + Send + 'static>,
    ) -> axum::http::Response<Self::ResponseBody> {
        let error_string = if let Some(s) = err.downcast_ref::<String>() {
            tracing::error!("Service panicked: {}", s);
            s.as_str()
        } else if let Some(s) = err.downcast_ref::<&str>() {
            tracing::error!("Service panicked: {}", s);
            s
        } else {
            let s = "Service panicked but `CatchPanic` was unable to downcast the panic info";
            tracing::error!("{}", s);
            s
        };

        Error(StatusCode::INTERNAL_SERVER_ERROR, eyre!("{}", error_string)).into_response()
    }
}

pub trait WithStatusCode<T> {
    fn with_status_code(self, code: StatusCode) -> Result<T>;
}

impl<T> WithStatusCode<T> for std::result::Result<T, color_eyre::eyre::Report> {
    fn with_status_code(self, code: StatusCode) -> Result<T> {
        self.map_err(|e| Error(code, e))
    }
}
