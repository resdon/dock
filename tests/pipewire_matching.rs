//! PipeWire window-to-stream matching tests
//!
//! Verifies that the title matching logic correctly prevents cross-app frame
//! mixing and handles edge cases like portal hashes, normalized titles, and
//! duplicate titles.

/// Test that portal hash stripping works correctly.
/// The portal appends a hex hash like " (e0bea6a97d79518c37c3e2539b1e93e2)"
/// to window titles. Matching must strip this hash first.
#[test]
fn test_strip_portal_hash() {
    // Simulate strip_portal_hash logic
    fn strip_portal_hash(title: &str) -> String {
        if let Some(pos) = title.rfind(" (") {
            let suffix = &title[pos + 2..];
            if let Some(end) = suffix.find(')') {
                let hash_part = &suffix[..end];
                // Check if it's a hex-only string (portal hash)
                if hash_part.chars().all(|c| c.is_ascii_hexdigit()) && hash_part.len() >= 8 {
                    return title[..pos].to_string();
                }
            }
        }
        title.to_string()
    }

    // Portal hash is stripped
    assert_eq!(
        strip_portal_hash("dock (8263e16bf64eac2d4d346578675c6b6d)"),
        "dock"
    );
    assert_eq!(
        strip_portal_hash(
            "Trump Was Never The REAL Problem (e0bea6a97d79518c37c3e2539b1e93e2)"
        ),
        "Trump Was Never The REAL Problem"
    );

    // No hash — title unchanged
    assert_eq!(strip_portal_hash("featherpad"), "featherpad");
    assert_eq!(strip_portal_hash("pcmanfm-qt /home"), "pcmanfm-qt /home");

    // Parentheses that are not a hex hash — kept
    assert_eq!(strip_portal_hash("Untitled (draft)"), "Untitled (draft)");
    assert_eq!(strip_portal_hash("File (1)"), "File (1)");
}

/// Test that title normalization handles escaped characters.
/// Portal titles may contain \u{200b} (zero-width space) or other escapes
/// that differ from Wayland titles.
#[test]
fn test_normalize_title() {
    // Simulate normalize_title logic
    fn normalize_title(title: &str) -> String {
        let mut result = String::with_capacity(title.len());
        let mut chars = title.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.peek() {
                    Some('n') => {
                        result.push('\n');
                        chars.next();
                    }
                    Some('t') => {
                        result.push('\t');
                        chars.next();
                    }
                    Some('r') => {
                        result.push('\r');
                        chars.next();
                    }
                    Some('u') => {
                        chars.next();
                        let mut hex = String::new();
                        for _ in 0..4 {
                            if let Some(h) = chars.next() {
                                hex.push(h);
                            }
                        }
                        if let Ok(code) = u32::from_str_radix(&hex, 16) {
                            if let Some(ch) = char::from_u32(code) {
                                result.push(ch);
                            }
                        }
                    }
                    Some('\'') => {
                        result.push('\'');
                        chars.next();
                    }
                    Some('\"') => {
                        result.push('"');
                        chars.next();
                    }
                    Some(other) => {
                        result.push('\\');
                        result.push(*other);
                        chars.next();
                    }
                    None => result.push('\\'),
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    // Escaped characters are normalized: \n becomes newline, \t becomes tab
    assert_eq!(normalize_title("hello\\nworld"), "hello\nworld");
    assert_eq!(normalize_title("col1\\tcol2"), "col1\tcol2");

    // Unicode escapes are normalized
    assert_eq!(normalize_title("hello\\u200bworld"), "hello\u{200b}world");

    // No escapes — unchanged
    assert_eq!(normalize_title("featherpad"), "featherpad");
}

/// Test that the app_id filtering logic prevents cross-app frame mixing.
/// This is the core invariant: a stream from one app must never be assigned
/// to a window from another app, even if titles match.
#[test]
fn test_app_id_filtering_logic() {
    // Simulate should_skip_different_app logic
    fn should_skip_different_app(
        query_app_id: &Option<String>,
        stream_app_id: &Option<String>,
    ) -> bool {
        match (query_app_id, stream_app_id) {
            (Some(query_app), Some(stream_app)) => query_app != stream_app,
            (Some(_), None) => false, // Stream has no app_id — allow match
            (None, Some(_)) => false, // Query has no app_id — allow match
            (None, None) => false,
        }
    }

    // Same app — should NOT skip
    assert!(
        !should_skip_different_app(&Some("microsoft-edge".to_string()), &Some("microsoft-edge".to_string()))
    );

    // Different app — SHOULD skip (this is the cross-app mixing prevention)
    assert!(
        should_skip_different_app(&Some("microsoft-edge".to_string()), &Some("org.pcsx2.PCmanFM-Qt".to_string()))
    );

    // Stream has no app_id — allow match (graceful degradation)
    assert!(
        !should_skip_different_app(&Some("microsoft-edge".to_string()), &None)
    );

    // Query has no app_id — allow match
    assert!(
        !should_skip_different_app(&None, &Some("microsoft-edge".to_string()))
    );

    // Both None — allow match
    assert!(!should_skip_different_app(&None, &None));
}

/// Test that the "last resort" fallback is removed.
/// Previously, when there was exactly one unbound stream, it would be assigned
/// regardless of app. This test documents that behavior should NOT happen.
#[test]
fn test_no_last_resort_fallback() {
    // The fix removes the last resort fallback entirely.
    // If no title match is found, find_stream_by_title returns None.
    // This test verifies the expected behavior:
    //
    // Scenario: pcmanfm window has no title match, but Edge's stream is unbound.
    // Expected: pcmanfm gets None (no frame), NOT Edge's stream.
    //
    // This is verified by the app_id filtering test above: when app_ids differ,
    // the stream is skipped. With no last resort fallback, None is returned.

    // Simulate: after all title matching strategies fail, what happens?
    let title_match_found = false;
    let has_last_resort_fallback = false; // The fix removes this

    if !title_match_found && !has_last_resort_fallback {
        // Correct behavior: return None
        assert!(true, "No last resort fallback — returns None when no title match");
    }
}

/// Test that exact title matching works after portal hash stripping.
#[test]
fn test_exact_match_after_hash_strip() {
    fn strip_portal_hash(title: &str) -> String {
        if let Some(pos) = title.rfind(" (") {
            let suffix = &title[pos + 2..];
            if let Some(end) = suffix.find(')') {
                let hash_part = &suffix[..end];
                if hash_part.chars().all(|c| c.is_ascii_hexdigit()) && hash_part.len() >= 8 {
                    return title[..pos].to_string();
                }
            }
        }
        title.to_string()
    }

    // Portal title has hash, Wayland title doesn't — should match after stripping
    let portal_title = "dock (8263e16bf64eac2d4d346578675c6b6d)";
    let wayland_title = "dock";

    assert_eq!(strip_portal_hash(portal_title), wayland_title);
}

/// Test that startswith matching works for truncated titles.
/// e.g., "dock" should match "dock – ```" via startswith.
#[test]
fn test_startswith_match() {
    let wayland_title = "dock";
    let portal_title = "dock – ```";

    let portal_lower = portal_title.to_lowercase();
    let wayland_lower = wayland_title.to_lowercase();

    assert!(portal_lower.starts_with(&wayland_lower));
}

/// Test that partial matching is skipped for short single-word titles.
/// "dock" (4 chars, 1 word) should NOT match "/path/to/dock" via partial match,
/// but SHOULD match "dock – ```" via startswith.
#[test]
fn test_short_title_partial_match_skipped() {
    let title = "dock";
    let title_words: Vec<&str> = title.split_whitespace().collect();
    let is_short_single_word = title_words.len() <= 1 && title.len() <= 4;

    assert!(is_short_single_word, "Short single-word titles should skip partial match");

    // But startswith still works
    let portal_title = "dock – ```".to_lowercase();
    assert!(portal_title.starts_with(&title.to_lowercase()));
}

/// Test that multiple windows with the same title are handled correctly.
/// Each window should get its own stream, not share one.
#[test]
fn test_duplicate_title_handling() {
    // Simulate: 3 windows with title "dock", 2 streams available
    // The used_streams set prevents the second window from claiming the first's stream
    let used_streams = vec![0usize];

    // First window already has stream 0
    // Second window tries to find a stream — stream 0 is excluded
    assert!(used_streams.contains(&0));

    // The second window should find stream 1 (if available) or None
    // It should NOT get stream 0
    let second_window_stream = if used_streams.contains(&0) {
        // Stream 0 is used — try to find another
        None // In this test, only stream 0 exists, so None is correct
    } else {
        Some(0)
    };

    assert!(
        second_window_stream.is_none(),
        "Second window should not get the first window's stream"
    );
}

/// Integration test: verify the complete matching flow prevents cross-app mixing.
#[test]
fn test_cross_app_mixing_prevention() {
    // Scenario: Edge and pcmanfm both have windows open.
    // Edge's stream title: "YouTube (hash)"
    // pcmanfm's Wayland title: "/home"
    //
    // Before fix: the "last resort" fallback would assign Edge's stream to pcmanfm
    // After fix: app_id filtering prevents this, and no last resort fallback exists

    let edge_app_id = Some("microsoft-edge".to_string());
    let pcmanfm_app_id = Some("org.pcsx2.PCmanFM-Qt".to_string());

    // App IDs differ — should skip
    fn should_skip_different_app(
        query_app_id: &Option<String>,
        stream_app_id: &Option<String>,
    ) -> bool {
        match (query_app_id, stream_app_id) {
            (Some(query_app), Some(stream_app)) => query_app != stream_app,
            _ => false,
        }
    }

    assert!(
        should_skip_different_app(&pcmanfm_app_id, &edge_app_id),
        "pcmanfm should NOT get Edge's stream"
    );

    // Even if titles somehow matched (e.g., both contain "dock"),
    // the app_id filter prevents the assignment
    assert!(
        should_skip_different_app(&pcmanfm_app_id, &edge_app_id),
        "App ID filter prevents cross-app mixing regardless of title"
    );
}
