use opentelemetry::trace::TraceContextExt;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::runtime::Tokio;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Initialize OpenTelemetry tracing with OTLP export.
/// Falls back to plain fmt tracing if OTEL_EXPORTER_OTLP_ENDPOINT is not set.
pub fn init_telemetry(service_name: &str) -> Option<()> {
    let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();

    if otlp_endpoint.is_none() {
        return None;
    }

    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(otlp_endpoint.unwrap());

    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(exporter)
        .with_trace_config(
            opentelemetry_sdk::trace::Config::default()
                .with_resource(opentelemetry_sdk::Resource::new(vec![
                    opentelemetry::KeyValue::new("service.name", service_name.to_string()),
                ])),
        )
        .install_batch(Tokio)
        .expect("Failed to install OpenTelemetry tracer");

    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "xfiles=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .with(telemetry)
        .init();

    tracing::info!("OpenTelemetry tracing initialized for {}", service_name);
    Some(())
}

/// Extract trace context from message headers.
pub fn trace_context_from_headers(
    headers: &std::collections::HashMap<String, String>,
) -> Option<opentelemetry::trace::SpanContext> {
    let trace_id = headers.get("x-trace-id")?;
    let span_id = headers.get("x-span-id")?;

    let trace_id = opentelemetry::trace::TraceId::from_hex(trace_id).ok()?;
    let span_id = opentelemetry::trace::SpanId::from_hex(span_id).ok()?;

    Some(opentelemetry::trace::SpanContext::new(
        trace_id,
        span_id,
        opentelemetry::trace::TraceFlags::SAMPLED,
        false,
        opentelemetry::trace::TraceState::default(),
    ))
}

/// Inject trace context into message headers.
pub fn inject_trace_context(
    span: &tracing::Span,
    headers: &mut std::collections::HashMap<String, String>,
) {
    use tracing_opentelemetry::OpenTelemetrySpanExt;
    let cx = span.context();
    let span_ref = cx.span();
    let span_context = span_ref.span_context();
    if span_context.is_valid() {
        headers.insert("x-trace-id".into(), span_context.trace_id().to_string());
        headers.insert("x-span-id".into(), span_context.span_id().to_string());
    }
}
