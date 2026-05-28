#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionClass {
    /// HTTP handler or Kafka consumer: entry and exit logs required
    Boundary,
    /// Makes external calls (HTTP, DB, Kafka): entry log, exit with duration required
    External,
    /// Internal function: exit log required on all return paths
    Internal,
}

/// Classify a function based on its signature and body.
pub fn classify_function(signature: &str, body: &str) -> FunctionClass {
    // Check for boundary functions
    if is_http_handler(signature) || is_kafka_consumer(signature) {
        return FunctionClass::Boundary;
    }

    // Check for external call patterns
    if has_external_calls(body) {
        return FunctionClass::External;
    }

    FunctionClass::Internal
}

fn is_http_handler(sig: &str) -> bool {
    // HTTP handler: (http.ResponseWriter, *http.Request)
    sig.contains("http.ResponseWriter") && sig.contains("*http.Request")
}

fn is_kafka_consumer(sig: &str) -> bool {
    // Kafka consumer: (context.Context, kafka.Message) or similar
    sig.contains("context.Context") && (sig.contains("kafka.Message") || sig.contains("Message"))
}

fn has_external_calls(body: &str) -> bool {
    // Check for HTTP client calls
    if body.contains("http.Do") || body.contains("http.Get") || body.contains("http.Post") {
        return true;
    }

    // Check for database calls
    if body.contains("sql.Query") || body.contains("sql.Exec") || body.contains("db.Query") {
        return true;
    }

    // Check for Kafka produce
    if body.contains(".Send(") && body.contains("Topic") {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_handler_classification() {
        let sig = "func (h *Handler) GetDoc(w http.ResponseWriter, r *http.Request) {";
        assert_eq!(classify_function(sig, ""), FunctionClass::Boundary);
    }

    #[test]
    fn test_external_function_classification() {
        let sig = "func fetchData(ctx context.Context) {";
        let body = "resp, err := http.Get(url)";
        assert_eq!(classify_function(sig, body), FunctionClass::External);
    }

    #[test]
    fn test_internal_function_classification() {
        let sig = "func helper(x int) int {";
        let body = "return x * 2";
        assert_eq!(classify_function(sig, body), FunctionClass::Internal);
    }
}
