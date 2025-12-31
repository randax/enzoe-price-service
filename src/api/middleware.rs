use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use axum::{
    body::Body,
    extract::Request,
    http::header::HeaderValue,
    response::Response,
};
use tower::{Layer, Service};
use uuid::Uuid;

use crate::metrics;

#[derive(Clone, Debug)]
pub struct CorrelationId(pub String);

impl CorrelationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct CorrelationIdLayer;

impl<S> Layer<S> for CorrelationIdLayer {
    type Service = CorrelationIdMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CorrelationIdMiddleware { inner }
    }
}

#[derive(Clone)]
pub struct CorrelationIdMiddleware<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for CorrelationIdMiddleware<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request<Body>) -> Self::Future {
        let correlation_id = req
            .headers()
            .get("X-Correlation-Id")
            .and_then(|v| v.to_str().ok())
            .map(|s| CorrelationId(s.to_string()))
            .unwrap_or_else(CorrelationId::new);

        req.extensions_mut().insert(correlation_id.clone());

        let mut inner = self.inner.clone();
        Box::pin(async move {
            let mut response = inner.call(req).await?;
            response.headers_mut().insert(
                "X-Correlation-Id",
                HeaderValue::from_str(&correlation_id.0).unwrap_or_else(|_| HeaderValue::from_static("unknown")),
            );
            Ok(response)
        })
    }
}

#[derive(Clone)]
pub struct MetricsLayer;

impl<S> Layer<S> for MetricsLayer {
    type Service = MetricsMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        MetricsMiddleware { inner }
    }
}

#[derive(Clone)]
pub struct MetricsMiddleware<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for MetricsMiddleware<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let start = Instant::now();
        let method = req.method().to_string();
        let path = normalize_path(req.uri().path());

        let mut inner = self.inner.clone();
        Box::pin(async move {
            let response = inner.call(req).await?;
            let duration = start.elapsed();
            let status = response.status().as_u16();

            metrics::record_http_request(&method, &path, status, duration);

            Ok(response)
        })
    }
}

fn normalize_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    let mut normalized = Vec::new();

    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            if i == 0 || i == parts.len() - 1 {
                normalized.push(*part);
            }
            continue;
        }

        if i > 0 {
            let prev = parts.get(i - 1).unwrap_or(&"");
            if *prev == "zone" || *prev == "country" {
                normalized.push(":id");
                continue;
            }
        }

        normalized.push(*part);
    }

    normalized.join("/")
}
