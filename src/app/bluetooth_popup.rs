//! Bluetooth popup hit-test helpers.
//!
//! The Bluetooth icon on the widget bar toggles a subsurface popup.
//! The Bluetooth icon on the dashed block spawns the `bluetooth-widget-complete`
//! application as a separate process — no rendering needed in the dock.
//!
//! This module provides hit-test functions for the subsurface popup buttons
//! (minimize, power, scan, device actions).

use crate::models::{BluetoothAction, BluetoothSnapshot};
use crate::render::bluetooth::{
    DEVICE_ACTION_H, DEVICE_ACTION_W, DEVICE_ACTION_X, DEVICE_ACTION_Y_OFFSET, DEVICE_LIST_Y,
    DEVICE_ROW_COUNT, HEADER_MINIMIZE_H, HEADER_MINIMIZE_W, HEADER_MINIMIZE_X, HEADER_MINIMIZE_Y,
    PAIRED_LIST_X, PAIRED_LIST_Y, POWER_BUTTON_H, POWER_BUTTON_W, POWER_BUTTON_X, POWER_BUTTON_Y,
    SCAN_BUTTON_H, SCAN_BUTTON_W, SCAN_BUTTON_X, SCAN_BUTTON_Y,
};

pub fn minimize_button_at(x: f64, y: f64) -> bool {
    let x = x.round() as i32;
    let y = y.round() as i32;
    (HEADER_MINIMIZE_X..HEADER_MINIMIZE_X + HEADER_MINIMIZE_W).contains(&x)
        && (HEADER_MINIMIZE_Y..HEADER_MINIMIZE_Y + HEADER_MINIMIZE_H).contains(&y)
}

/// Hit-test the power/scan buttons and the per-device Pair/Connect/
/// Disconnect actions in the two-column layout (DISCOVERED left, PAIRED right).
/// Each action contains the exact BlueZ device object path from the latest
/// ObjectManager snapshot.
pub fn action_for_click(snapshot: &BluetoothSnapshot, x: f64, y: f64) -> Option<BluetoothAction> {
    if !snapshot.available || snapshot.adapter_path.is_none() {
        return None;
    }
    let x = x.round() as i32;
    let y = y.round() as i32;

    // Power toggle card
    if (POWER_BUTTON_Y..POWER_BUTTON_Y + POWER_BUTTON_H).contains(&y)
        && (POWER_BUTTON_X..POWER_BUTTON_X + POWER_BUTTON_W).contains(&x)
    {
        return Some(BluetoothAction::SetPowered(!snapshot.powered));
    }
    // Scan toggle card
    if (SCAN_BUTTON_Y..SCAN_BUTTON_Y + SCAN_BUTTON_H).contains(&y)
        && (SCAN_BUTTON_X..SCAN_BUTTON_X + SCAN_BUTTON_W).contains(&x)
        && snapshot.powered
    {
        return Some(BluetoothAction::SetDiscovering(!snapshot.discovery_owned));
    }

    // Separate devices into discovered (not paired) and paired
    let discovered: Vec<_> = snapshot.devices.iter().filter(|d| !d.paired).collect();
    let paired: Vec<_> = snapshot.devices.iter().filter(|d| d.paired).collect();

    // Hit-test DISCOVERED list (left column, x=22, y=154)
    for (idx, device) in discovered.iter().take(DEVICE_ROW_COUNT).enumerate() {
        let card_y = DEVICE_LIST_Y + idx as i32 * 43;
        let action_x = 22 + DEVICE_ACTION_X; // card x + action offset
        let action_y = card_y + DEVICE_ACTION_Y_OFFSET;
        if (action_y..action_y + DEVICE_ACTION_H).contains(&y)
            && (action_x..action_x + DEVICE_ACTION_W).contains(&x)
        {
            return Some(if device.connected {
                BluetoothAction::Disconnect(device.path.clone())
            } else {
                BluetoothAction::Pair(device.path.clone())
            });
        }
    }

    // Hit-test PAIRED list (right column, x=208, y=111)
    for (idx, device) in paired.iter().take(4).enumerate() {
        let card_y = PAIRED_LIST_Y + idx as i32 * 43;
        let action_x = PAIRED_LIST_X + DEVICE_ACTION_X; // card x + action offset
        let action_y = card_y + DEVICE_ACTION_Y_OFFSET;
        if (action_y..action_y + DEVICE_ACTION_H).contains(&y)
            && (action_x..action_x + DEVICE_ACTION_W).contains(&x)
        {
            return Some(if device.connected {
                BluetoothAction::Disconnect(device.path.clone())
            } else {
                BluetoothAction::Connect(device.path.clone())
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BluetoothDevice;

    fn sample() -> BluetoothSnapshot {
        BluetoothSnapshot {
            available: true,
            adapter_path: Some("/org/bluez/hci0".into()),
            adapter_name: "Arch Linux".into(),
            powered: true,
            devices: vec![
                BluetoothDevice {
                    path: "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF".into(),
                    address: "AA:BB:CC:DD:EE:FF".into(),
                    name: "Headphones".into(),
                    paired: true,
                    connected: true,
                    trusted: true,
                },
                BluetoothDevice {
                    path: "/org/bluez/hci0/dev_11_22_33_44_55_66".into(),
                    address: "11:22:33:44:55:66".into(),
                    name: "Keyboard".into(),
                    paired: true,
                    connected: false,
                    trusted: true,
                },
                BluetoothDevice {
                    path: "/org/bluez/hci0/dev_77_88_99_AA_BB_CC".into(),
                    address: "77:88:99:AA:BB:CC".into(),
                    name: "New device".into(),
                    paired: false,
                    connected: false,
                    trusted: false,
                },
            ],
            ..BluetoothSnapshot::default()
        }
    }

    #[test]
    fn power_button_toggles_power_state() {
        assert!(matches!(
            action_for_click(&sample(), 20.0, 35.0),
            Some(BluetoothAction::SetPowered(false))
        ));
    }

    #[test]
    fn scan_button_starts_discovery_when_powered() {
        assert!(matches!(
            action_for_click(&sample(), 150.0, 35.0),
            Some(BluetoothAction::SetDiscovering(true))
        ));
    }

    #[test]
    fn device_rows_offer_disconnect_connect_and_pair() {
        let snapshot = sample();
        assert!(matches!(
            action_for_click(&snapshot, 180.0, 80.0),
            Some(BluetoothAction::Disconnect(_))
        ));
        assert!(matches!(
            action_for_click(&snapshot, 180.0, 118.0),
            Some(BluetoothAction::Connect(_))
        ));
        assert!(matches!(
            action_for_click(&snapshot, 180.0, 156.0),
            Some(BluetoothAction::Pair(_))
        ));
    }

    #[test]
    fn minimize_button_is_only_header_hitbox() {
        assert!(minimize_button_at(200.0, 10.0));
        assert!(!minimize_button_at(150.0, 10.0));
    }
}
