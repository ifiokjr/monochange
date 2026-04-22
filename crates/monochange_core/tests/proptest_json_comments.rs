use monochange_core::strip_json_comments;
use proptest::prelude::*;

proptest! {
	/// Idempotence: applying strip_json_comments twice has the same effect as once.
	#[test]
	fn strip_json_comments_is_idempotent_for_ascii(
		s in "[\x00-\x7f]{0,128}"
	) {
		let once = strip_json_comments(&s);
		let twice = strip_json_comments(&once);
		prop_assert_eq!(once, twice, "strip_json_comments should be idempotent");
	}

	/// Strings without `//` or `/*` pass through unchanged.
	#[test]
	fn strip_json_comments_preserves_comment_free_strings(
		s in "[\x00-.0-\x7f]*"
	) {
		let result = strip_json_comments(&s);
		prop_assert_eq!(result, s, "comment-free string was modified");
	}

	/// Line comments are removed while preserving trailing content.
	#[test]
	fn strip_json_comments_removes_line_comments(
		pre in "[a-z0-9]*",
		comment in "[a-z0-9 ]*",
		post in "[a-z0-9]*"
	) {
		let input = format!("{pre}//{comment}\n{post}");
		let result = strip_json_comments(&input);
		let expected = format!("{pre}\n{post}");
		prop_assert_eq!(result, expected, "line comment was not removed");
	}

	/// Block comments are removed while preserving trailing content.
	#[test]
	fn strip_json_comments_removes_block_comments(
		pre in "[a-z0-9]*",
		comment in "[a-z0-9 ]*",
		post in "[a-z0-9]*"
	) {
		let input = format!("{pre}/*{comment}*/{post}");
		let result = strip_json_comments(&input);
		let expected = format!("{pre}{post}");
		prop_assert_eq!(result, expected, "block comment was not removed");
	}

	/// Content inside double-quoted strings is preserved, including `//` and `/*`.
	#[test]
	fn strip_json_comments_preserves_comments_inside_strings(
		key in "[a-z]{1,10}",
		value in "[a-z ]{0,20}",
	) {
		let input = format!(r#"{{"{key}": "// not a comment /* also not */ {value}"}}"#);
		let result = strip_json_comments(&input);
		prop_assert_eq!(result, input, "content inside strings was corrupted");
	}
}
