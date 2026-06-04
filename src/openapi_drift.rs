pub const CLI_KNOWN_PATHS: &[&str] = &[
    "/v1/health",
    "/v1/version",
    "/v1/capabilities",
    "/v1/tasks",
    "/v1/tasks/{id}",
    "/v1/tasks/{id}/complete",
    "/v1/projects",
    "/v1/reviews/days/{date}",
    "/v1/reviews/weeks/{weekStart}",
    "/v1/reviews/weeks/{weekStart}/values/{valueId}",
    "/v1/ai-agent/tools",
    "/v1/ai-agent/tasks/{id}/export",
    "/v1/ai-agent/tasks/validate",
    "/v1/ai-agent/tasks/import",
];

#[allow(dead_code)]
pub const CRITICAL_OPENAPI_PATHS: &[&str] = &[
    "/",
    "/v1/health",
    "/v1/version",
    "/v1/capabilities",
    "/v1/commands",
    "/v1/tasks",
    "/v1/tasks/{id}",
    "/v1/tasks/{id}/complete",
    "/v1/projects",
    "/v1/projects/{id}",
    "/v1/reviews/days/{date}",
    "/v1/reviews/weeks/{weekStart}",
    "/v1/reviews/weeks/{weekStart}/values/{valueId}",
    "/v1/ai-agent/tools",
    "/v1/ai-agent/tasks/{id}/export",
    "/v1/ai-agent/tasks/validate",
    "/v1/ai-agent/tasks/import",
];

#[allow(dead_code)]
pub const INTENTIONALLY_UNSUPPORTED_OPENAPI_PATHS: &[&str] =
    &["/", "/v1/commands", "/v1/projects/{id}"];
