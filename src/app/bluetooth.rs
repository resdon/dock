//! Bluetooth monitor service.
//!
//! Monitors BlueZ D-Bus for device state changes and connection status.

use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use zbus::zvariant::{OwnedObjectPath, OwnedVariant, Variant};
use zbus::{Connection, Proxy};

use crate::models::{BluetoothAction, BluetoothDevice, BluetoothSnapshot};

const BLUEZ_SERVICE: &str = "org.bluez";
const OBJECT_MANAGER_PATH: &str = "/";
const OBJECT_MANAGER_IFACE: &str = "org.freedesktop.DBus.ObjectManager";
const ADAPTER_IFACE: &str = "org.bluez.Adapter1";
const DEVICE_IFACE: &str = "org.bluez.Device1";
const PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";

type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedVariant>>>;

/// Keep all BlueZ calls off the Wayland/render thread. The monitor uses the
/// system D-Bus ObjectManager rather than parsing terminal output, so device
/// names and connection state come from BlueZ's authoritative properties.
pub fn spawn_bluetooth_monitor(
    snapshot_tx: std::sync::mpsc::Sender<BluetoothSnapshot>,
    mut action_rx: tokio::sync::mpsc::UnboundedReceiver<BluetoothAction>,
) {
    thread::Builder::new()
        .name("dock-bluez-monitor".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let _ = snapshot_tx.send(BluetoothSnapshot {
                        available: false,
                        error: Some(format!("Could not start Bluetooth monitor: {error}")),
                        ..BluetoothSnapshot::default()
                    });
                    return;
                }
            };

            runtime.block_on(async move {
                let mut connection: Option<Connection> = None;
                let mut last_snapshot: Option<BluetoothSnapshot> = None;
                let mut status_message: Option<String> = None;
                let mut status_expires: Option<tokio::time::Instant> = None;
                let mut discovery_owned = false;
                let mut poll = tokio::time::interval(Duration::from_millis(1400));
                poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                loop {
                    tokio::select! {
                        _ = poll.tick() => {
                            if connection.is_none() {
                                match Connection::system().await {
                                    Ok(conn) => {
                                        connection = Some(conn);
                                    }
                                    Err(error) => {
                                        let snapshot = BluetoothSnapshot {
                                            available: false,
                                            error: Some(format!("System D-Bus unavailable: {error}")),
                                            ..BluetoothSnapshot::default()
                                        };
                                        if last_snapshot.as_ref() != Some(&snapshot) {
                                            let _ = snapshot_tx.send(snapshot.clone());
                                            last_snapshot = Some(snapshot);
                                        }
                                        continue;
                                    }
                                }
                            }

                            let Some(conn) = connection.as_ref() else { continue; };
                            let mut snapshot = read_snapshot(conn).await;
                            snapshot.discovery_owned = discovery_owned;
                            if status_expires.is_some_and(|expires| tokio::time::Instant::now() >= expires) {
                                status_message = None;
                                status_expires = None;
                            }
                            snapshot.status_message = status_message.clone();
                            if last_snapshot.as_ref() != Some(&snapshot) {
                                if snapshot_tx.send(snapshot.clone()).is_err() {
                                    break;
                                }
                                last_snapshot = Some(snapshot);
                            }
                        }
                        Some(action) = action_rx.recv() => {
                            if connection.is_none() {
                                connection = Connection::system().await.ok();
                            }
                            let result = match connection.as_ref() {
                                Some(conn) => apply_action(conn, action, &mut discovery_owned).await,
                                None => Err("BlueZ is unavailable on the system D-Bus".to_string()),
                            };
                            status_message = Some(match result {
                                Ok(message) => message,
                                Err(error) => {
                                    format!("Error: {}", compact_error(&error))
                                }
                            });
                            status_expires = Some(tokio::time::Instant::now() + Duration::from_secs(5));

                            if let Some(conn) = connection.as_ref() {
                                let mut snapshot = read_snapshot(conn).await;
                                snapshot.discovery_owned = discovery_owned;
                                snapshot.status_message = status_message.clone();
                                if last_snapshot.as_ref() != Some(&snapshot) {
                                    if snapshot_tx.send(snapshot.clone()).is_err() {
                                        break;
                                    }
                                    last_snapshot = Some(snapshot);
                                }
                            }
                        }
                        else => break,
                    }
                }
            });
        })
        .ok();
}

async fn read_snapshot(connection: &Connection) -> BluetoothSnapshot {
    let proxy = match Proxy::new(
        connection,
        BLUEZ_SERVICE,
        OBJECT_MANAGER_PATH,
        OBJECT_MANAGER_IFACE,
    )
    .await
    {
        Ok(proxy) => proxy,
        Err(error) => {
            return BluetoothSnapshot {
                available: false,
                error: Some(format!("Could not access BlueZ ObjectManager: {error}")),
                ..BluetoothSnapshot::default()
            };
        }
    };

    let objects: ManagedObjects = match proxy.call("GetManagedObjects", &()).await {
        Ok(objects) => objects,
        Err(error) => {
            return BluetoothSnapshot {
                available: false,
                error: Some(format!("BlueZ service unavailable: {error}")),
                ..BluetoothSnapshot::default()
            };
        }
    };

    let mut snapshot = BluetoothSnapshot {
        available: true,
        ..BluetoothSnapshot::default()
    };

    for (object_path, interfaces) in &objects {
        if let Some(properties) = interfaces.get(ADAPTER_IFACE) {
            // Prefer a powered controller, but keep an adapter selected when
            // it is currently switched off so the UI can turn it back on.
            let powered = value_bool(properties, "Powered").unwrap_or(false);
            if snapshot.adapter_path.is_none() || (powered && !snapshot.powered) {
                snapshot.adapter_path = Some(object_path.to_string());
                snapshot.adapter_name = value_string(properties, "Alias")
                    .or_else(|| value_string(properties, "Name"))
                    .unwrap_or_else(|| "Bluetooth adapter".into());
                snapshot.powered = powered;
                snapshot.discovering = value_bool(properties, "Discovering").unwrap_or(false);
            }
        }

        if let Some(properties) = interfaces.get(DEVICE_IFACE) {
            let address = value_string(properties, "Address").unwrap_or_default();
            if address.is_empty() {
                continue;
            }
            let name = value_string(properties, "Alias")
                .or_else(|| value_string(properties, "Name"))
                .unwrap_or_else(|| address.clone());
            snapshot.devices.push(BluetoothDevice {
                path: object_path.to_string(),
                address,
                name,
                paired: value_bool(properties, "Paired").unwrap_or(false),
                connected: value_bool(properties, "Connected").unwrap_or(false),
                trusted: value_bool(properties, "Trusted").unwrap_or(false),
            });
        }
    }

    snapshot.devices.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then_with(|| b.paired.cmp(&a.paired))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    snapshot
}

async fn apply_action(
    connection: &Connection,
    action: BluetoothAction,
    discovery_owned: &mut bool,
) -> Result<String, String> {
    match action {
        BluetoothAction::SetPowered(powered) => {
            let snapshot = read_snapshot(connection).await;
            let path = snapshot
                .adapter_path
                .ok_or_else(|| "No Bluetooth adapter found".to_string())?;
            if !powered && *discovery_owned {
                // Release only this dock's scan session before powering down;
                // another bluetoothctl/Blueman session is left untouched.
                if let Ok(adapter) =
                    Proxy::new(connection, BLUEZ_SERVICE, path.as_str(), ADAPTER_IFACE).await
                {
                    let _: Result<(), _> = adapter.call("StopDiscovery", &()).await;
                }
                *discovery_owned = false;
            }
            set_property(
                connection,
                &path,
                ADAPTER_IFACE,
                "Powered",
                Variant::Bool(powered),
            )
            .await?;
            Ok(if powered {
                "Bluetooth enabled"
            } else {
                "Bluetooth disabled"
            }
            .into())
        }
        BluetoothAction::SetDiscovering(discovering) => {
            if discovering && *discovery_owned {
                return Ok("Scan already active".into());
            }
            if !discovering && !*discovery_owned {
                return Err("Scan is controlled by another app".into());
            }
            let snapshot = read_snapshot(connection).await;
            let path = snapshot
                .adapter_path
                .ok_or_else(|| "No Bluetooth adapter found".to_string())?;
            let adapter = Proxy::new(connection, BLUEZ_SERVICE, path.as_str(), ADAPTER_IFACE)
                .await
                .map_err(|e| e.to_string())?;
            if discovering {
                let _: () = adapter
                    .call("StartDiscovery", &())
                    .await
                    .map_err(|e| e.to_string())?;
                *discovery_owned = true;
                Ok("Scanning for devices".into())
            } else {
                let _: () = adapter
                    .call("StopDiscovery", &())
                    .await
                    .map_err(|e| e.to_string())?;
                *discovery_owned = false;
                Ok("Discovery stopped".into())
            }
        }
        BluetoothAction::Connect(path) => {
            let device = Proxy::new(connection, BLUEZ_SERVICE, path.as_str(), DEVICE_IFACE)
                .await
                .map_err(|e| e.to_string())?;
            let _: () = device
                .call("Connect", &())
                .await
                .map_err(|e| e.to_string())?;
            Ok("Device connected".into())
        }
        BluetoothAction::Disconnect(path) => {
            let device = Proxy::new(connection, BLUEZ_SERVICE, path.as_str(), DEVICE_IFACE)
                .await
                .map_err(|e| e.to_string())?;
            let _: () = device
                .call("Disconnect", &())
                .await
                .map_err(|e| e.to_string())?;
            Ok("Device disconnected".into())
        }
        BluetoothAction::Pair(path) => {
            let device = Proxy::new(connection, BLUEZ_SERVICE, path.as_str(), DEVICE_IFACE)
                .await
                .map_err(|e| e.to_string())?;
            let _: () = device.call("Pair", &()).await.map_err(|e| e.to_string())?;
            // Pairing does not implicitly trust a device via the raw BlueZ API.
            // Mark it trusted after the pairing method has completed.
            set_property(
                connection,
                &path,
                DEVICE_IFACE,
                "Trusted",
                Variant::Bool(true),
            )
            .await?;
            Ok("Device paired and trusted".into())
        }
    }
}

async fn set_property(
    connection: &Connection,
    object_path: &str,
    interface: &str,
    property: &str,
    value: Variant<'_>,
) -> Result<(), String> {
    let properties = Proxy::new(connection, BLUEZ_SERVICE, object_path, PROPERTIES_IFACE)
        .await
        .map_err(|e| e.to_string())?;
    let _: () = properties
        .call("Set", &(interface, property, value))
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn value_string(properties: &HashMap<String, OwnedVariant>, key: &str) -> Option<String> {
    properties
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| String::try_from(value).ok())
}

fn value_bool(properties: &HashMap<String, OwnedVariant>, key: &str) -> Option<bool> {
    properties
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| bool::try_from(value).ok())
}

fn compact_error(error: &str) -> String {
    let flattened = error.lines().next().unwrap_or(error).trim();
    if flattened.chars().count() <= 34 {
        flattened.to_string()
    } else {
        format!("{}…", flattened.chars().take(33).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_error_is_single_line_and_bounded() {
        assert_eq!(
            compact_error("Authentication failed\nmore detail"),
            "Authentication failed"
        );
        assert_eq!(
            compact_error("1234567890123456789012345678901234567890")
                .chars()
                .count(),
            34
        );
    }
}
