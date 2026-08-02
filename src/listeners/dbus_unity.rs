// src/listeners/dbus_unity.rs

use std::collections::HashMap;
use tokio::sync::mpsc;
use zbus::zvariant::Value;
use zbus::MatchRule;

use crate::models::BadgeUpdate;

pub async fn start_unity_dbus_listener(
    tx: mpsc::UnboundedSender<BadgeUpdate>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let connection = zbus::Connection::session().await?;

    let rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("com.canonical.Unity.LauncherEntry")?
        .member("Update")?
        .build();

    let mut stream = zbus::MessageStream::for_match_rule(rule, &connection, None).await?;

    tokio::spawn(async move {
        use futures_util::StreamExt;
        while let Some(Ok(msg)) = stream.next().await {
            if let Ok((app_uri, properties)) = msg.body().deserialize::<(String, HashMap<String, Value>)>() {
                if let Some(update) = parse_unity_update(app_uri, properties) {
                    let _ = tx.send(update);
                }
            }
        }
    });

    Ok(())
}

fn parse_unity_update(app_uri: String, props: HashMap<String, Value>) -> Option<BadgeUpdate> {
    let desktop_id = app_uri
        .strip_prefix("application://")
        .or_else(|| app_uri.rsplit('/').next())
        .unwrap_or(&app_uri)
        .trim_end_matches(".desktop")
        .to_string();

    if desktop_id.is_empty() {
        return None;
    }

    let mut update = BadgeUpdate {
        desktop_id,
        ..Default::default()
    };

    if let Some(val) = props.get("count") {
        update.count = match val {
            Value::I64(v) => *v,
            Value::I32(v) => *v as i64,
            Value::U64(v) => *v as i64,
            Value::U32(v) => *v as i64,
            _ => 0,
        };
    }

    if let Some(Value::Bool(v)) = props.get("count-visible") {
        update.count_visible = *v;
    } else {
        update.count_visible = update.count > 0;
    }

    if let Some(val) = props.get("progress") {
        update.progress = match val {
            Value::F64(v) => *v,
            Value::I64(v) => *v as f64 / 100.0,
            _ => 0.0,
        };
    }

    if let Some(Value::Bool(v)) = props.get("progress-visible") {
        update.progress_visible = *v;
    } else {
        update.progress_visible = update.progress > 0.0 && update.progress <= 1.0;
    }

    if let Some(Value::Bool(v)) = props.get("urgent") {
        update.urgent = *v;
    }

    Some(update)
}