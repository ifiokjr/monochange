pub fn content_text(result: &rmcp::model::CallToolResult) -> String {
	result
		.content
		.first()
		.and_then(|content| {
			match &content.raw {
				rmcp::model::RawContent::Text(text) => Some(text.text.clone()),
				_ => None,
			}
		})
		.unwrap_or_default()
}
