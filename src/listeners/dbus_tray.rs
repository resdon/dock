//! D-Bus system tray listener.
//!
//! Monitors StatusNotifierItem icons and provides tray menu actions.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use zbus::message::Header;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};
use zbus::{fdo, interface, Connection, Proxy};

use crate::models::{TrayAction, TrayActionKind, TrayIcon, TrayItem, TraySnapshot, TrayStatus};

const WATCHER_IFACE: &str = "org.freedesktop.StatusNotifierWatcher";
const WATCHER_KDE_IFACE: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_SERVICE: &str = "org.freedesktop.StatusNotifierWatcher";
const WATCHER_KDE_SERVICE: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM_IFACE: &str = "org.freedesktop.StatusNotifierItem";
const ITEM_KDE_IFACE: &str = "org.kde.StatusNotifierItem";
const PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";

/// Compatibility watcher for wlroots-style sessions.  We expose both the
/// freedesktop and KDE-compatible watcher names because a large number of
/// existing tray clients still probe/register against the KDE name.
struct LocalStatusNotifierWatcher {
    items: Vec<String>,
    hosts: HashSet<String>,
    registration_tx: tokio::sync::mpsc::UnboundedSender<()>,
}

impl LocalStatusNotifierWatcher {
    fn new(registration_tx: tokio::sync::mpsc::UnboundedSender<()>) -> Self {
        Self {
            items: Vec::new(),
            hosts: HashSet::new(),
            registration_tx,
        }
    }
}

#[interface(name = "org.freedesktop.StatusNotifierWatcher")]
impl LocalStatusNotifierWatcher {
    #[zbus(property, name = "RegisteredStatusNotifierItems")]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.clone()
    }

    #[zbus(property, name = "IsStatusNotifierHostRegistered")]
    fn is_status_notifier_host_registered(&self) -> bool {
        !self.hosts.is_empty()
    }

    #[zbus(property, name = "ProtocolVersion")]
    fn protocol_version(&self) -> u32 {
        0
    }

    async fn register_status_notifier_item(
        &mut self,
        service_name: String,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let reference = normalize_registered_item(&service_name, &header);
        if !reference.is_empty() && !self.items.iter().any(|item| item == &reference) {
            self.items.push(reference);
            let _ = self.registration_tx.send(());
        }
        Ok(())
    }

    async fn register_status_notifier_host(&mut self, service_name: String) -> fdo::Result<()> {
        if !service_name.is_empty() {
            self.hosts.insert(service_name);
        }
        Ok(())
    }
}

struct LocalKdeStatusNotifierWatcher {
    items: Vec<String>,
    hosts: HashSet<String>,
    registration_tx: tokio::sync::mpsc::UnboundedSender<()>,
}

impl LocalKdeStatusNotifierWatcher {
    fn new(registration_tx: tokio::sync::mpsc::UnboundedSender<()>) -> Self {
        Self {
            items: Vec::new(),
            hosts: HashSet::new(),
            registration_tx,
        }
    }
}

#[interface(name = "org.kde.StatusNotifierWatcher")]
impl LocalKdeStatusNotifierWatcher {
    #[zbus(property, name = "RegisteredStatusNotifierItems")]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.clone()
    }

    #[zbus(property, name = "IsStatusNotifierHostRegistered")]
    fn is_status_notifier_host_registered(&self) -> bool {
        !self.hosts.is_empty()
    }

    #[zbus(property, name = "ProtocolVersion")]
    fn protocol_version(&self) -> u32 {
        0
    }

    async fn register_status_notifier_item(
        &mut self,
        service_name: String,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let reference = normalize_registered_item(&service_name, &header);
        if !reference.is_empty() && !self.items.iter().any(|item| item == &reference) {
            self.items.push(reference);
            let _ = self.registration_tx.send(());
        }
        Ok(())
    }

    async fn register_status_notifier_host(&mut self, service_name: String) -> fdo::Result<()> {
        if !service_name.is_empty() {
            self.hosts.insert(service_name);
        }
        Ok(())
    }
}

fn normalize_registered_item(argument: &str, header: &Header<'_>) -> String {
    let argument = argument.trim();
    if argument.is_empty() {
        return String::new();
    }

    // SNI permits a client to register by object path only. In that form the
    // D-Bus sender in the method header is the service name we must use to
    // construct the item proxy.
    if argument.starts_with('/') {
        if let Some(sender) = header.sender() {
            return format!("{sender}{argument}");
        }
        return String::new();
    }

    argument.to_string()
}

pub async fn start_status_notifier_listener(
    snapshot_tx: tokio::sync::mpsc::UnboundedSender<TraySnapshot>,
    mut action_rx: tokio::sync::mpsc::UnboundedReceiver<TrayAction>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let initial_connection = Connection::session().await?;
    let dbus = fdo::DBusProxy::new(&initial_connection).await?;

    let fdo_owner = dbus
        .name_has_owner(WATCHER_SERVICE.try_into()?)
        .await
        .unwrap_or(false);
    let kde_owner = dbus
        .name_has_owner(WATCHER_KDE_SERVICE.try_into()?)
        .await
        .unwrap_or(false);

    let (registration_tx, mut registration_rx) = tokio::sync::mpsc::unbounded_channel::<()>();

    let (connection, using_local_watcher) = if fdo_owner || kde_owner {
        if fdo_owner && kde_owner {
        } else if fdo_owner {
        } else {
        }
        (initial_connection, false)
    } else {
        drop(dbus);
        let connection = zbus::connection::Builder::session()?
            .name(WATCHER_SERVICE)?
            .name(WATCHER_KDE_SERVICE)?
            .serve_at(
                WATCHER_PATH,
                LocalStatusNotifierWatcher::new(registration_tx.clone()),
            )?
            .serve_at(
                WATCHER_PATH,
                LocalKdeStatusNotifierWatcher::new(registration_tx.clone()),
            )?
            .build()
            .await?;
        (connection, true)
    };

    // Some wlroots sessions expose only one of the two well-known watcher
    // names. Several legacy clients (including Steam/Ayatana builds) probe the
    // KDE name directly, so provide the missing alias instead of silently
    // leaving those clients unable to register.
    let mut compatibility_connection: Option<Connection> = None;
    if fdo_owner && !kde_owner {
        compatibility_connection = Some(
            zbus::connection::Builder::session()?
                .name(WATCHER_KDE_SERVICE)?
                .serve_at(
                    WATCHER_PATH,
                    LocalKdeStatusNotifierWatcher::new(registration_tx.clone()),
                )?
                .build()
                .await?,
        );
    } else if kde_owner && !fdo_owner {
        compatibility_connection = Some(
            zbus::connection::Builder::session()?
                .name(WATCHER_SERVICE)?
                .serve_at(
                    WATCHER_PATH,
                    LocalStatusNotifierWatcher::new(registration_tx.clone()),
                )?
                .build()
                .await?,
        );
    }

    let mut watchers = Vec::with_capacity(2);
    if fdo_owner || using_local_watcher {
        if let Ok(proxy) =
            Proxy::new(&connection, WATCHER_SERVICE, WATCHER_PATH, WATCHER_IFACE).await
        {
            watchers.push(proxy);
        }
    } else if let Some(compat) = compatibility_connection.as_ref() {
        if let Ok(proxy) = Proxy::new(compat, WATCHER_SERVICE, WATCHER_PATH, WATCHER_IFACE).await {
            watchers.push(proxy);
        }
    }
    if kde_owner || using_local_watcher {
        if let Ok(proxy) = Proxy::new(
            &connection,
            WATCHER_KDE_SERVICE,
            WATCHER_PATH,
            WATCHER_KDE_IFACE,
        )
        .await
        {
            watchers.push(proxy);
        }
    } else if let Some(compat) = compatibility_connection.as_ref() {
        if let Ok(proxy) =
            Proxy::new(compat, WATCHER_KDE_SERVICE, WATCHER_PATH, WATCHER_KDE_IFACE).await
        {
            watchers.push(proxy);
        }
    }

    let host_name = format!("org.freedesktop.StatusNotifierHost-{}", std::process::id());
    for watcher in &watchers {
        if let Err(e) = watcher
            .call_noreply("RegisterStatusNotifierHost", &(host_name.as_str(),))
            .await
        {
        }
    }

    // External watcher implementations do not all emit host-visible change
    // signals consistently. A modest 5s poll keeps late-registered Qt tray
    // items (notably qBittorrent) from remaining invisible indefinitely.
    let mut refresh_interval = tokio::time::interval(Duration::from_secs(5));
    refresh_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut items: HashMap<String, TrayItem> = HashMap::new();
    if refresh_all_items(&watchers, &connection, &mut items).await {
        publish_snapshot(&snapshot_tx, &items);
    } else {
    }

    loop {
        tokio::select! {
            Some(()) = registration_rx.recv() => {
                // A local watcher receives registrations in real time. Refresh
                // immediately instead of waiting for the 10-second fallback poll.
                if refresh_all_items(&watchers, &connection, &mut items).await {
                    publish_snapshot(&snapshot_tx, &items);
                }
            }
            _ = refresh_interval.tick() => {
                // Tray properties/status can change without a registration change.
                // Refresh the watcher plus qBittorrent/Qt fallback names, and only
                // publish when the actual snapshot changed.
                if refresh_all_items(&watchers, &connection, &mut items).await {
                    publish_snapshot(&snapshot_tx, &items);
                }
            }
            Some(action) = action_rx.recv() => {
                perform_action(&connection, &action).await;
            }
            else => break,
        }
    }

    Ok(())
}

async fn refresh_all_items(
    watchers: &[Proxy<'_>],
    connection: &Connection,
    items: &mut HashMap<String, TrayItem>,
) -> bool {
    let mut registered = HashSet::<String>::new();

    // First use the official watcher registry. Querying both implementations is
    // cheap and makes the host compatible with both freedesktop and KDE clients.
    for watcher in watchers {
        match watcher
            .get_property::<Vec<String>>("RegisteredStatusNotifierItems")
            .await
        {
            Ok(list) => registered.extend(list),
            Err(error) if std::env::var_os("DOCK_TRAY_DEBUG").is_some() => {
            }
            Err(_) => {}
        }
    }

    if std::env::var_os("DOCK_TRAY_DEBUG").is_some() {
        // Debug: sort and print registered refs
    }

    let mut next = HashMap::with_capacity(registered.len());
    let mut loaded_services = HashSet::new();
    for reference in registered {
        // Once an item has been identified, keep using its working interface
        // first. This avoids the expensive/failed freedesktop probe for KDE SNI
        // clients on every refresh (notably qBittorrent).
        let preferred_interface = known_interface_for_reference(items, &reference);
        if let Some(item) = load_tray_item(connection, &reference, preferred_interface).await {
            loaded_services.insert(item.service_name.clone());
            next.insert(item.key.clone(), item);
        } else {
            if let Some(existing) = find_existing_item_for_reference(items, &reference) {
                // A transient D-Bus error must not make a visible tray icon disappear.
                loaded_services.insert(existing.service_name.clone());
                next.insert(existing.key.clone(), existing.clone());
            }
        }
    }

    // A few Qt/AppIndicator builds have been observed to own a conventional
    // SNI bus name while their watcher registry is stale or incomplete. Probe
    // only names strongly associated with tray items, and accept them only if
    // they actually export a StatusNotifierItem interface.
    for service_name in discover_sni_service_names(connection).await {
        if loaded_services.contains(&service_name) {
            continue;
        }
        if let Some(item) = load_tray_item(connection, &service_name, None).await {
            next.insert(item.key.clone(), item);
        }
    }

    // Stable rendering order avoids tray items jumping around when the D-Bus
    // registry is refreshed from a HashSet.
    let mut ordered: Vec<TrayItem> = next.into_values().collect();
    ordered.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.service_name.cmp(&b.service_name))
            .then_with(|| a.object_path.cmp(&b.object_path))
    });

    let next: HashMap<String, TrayItem> = ordered
        .into_iter()
        .map(|item| (item.key.clone(), item))
        .collect();

    let changed = tray_items_differ(items, &next);
    *items = next;
    changed
}

async fn discover_sni_service_names(connection: &Connection) -> Vec<String> {
    let Ok(proxy) = Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await
    else {
        return Vec::new();
    };
    let Ok(names) = proxy.call::<_, _, Vec<String>>("ListNames", &()).await else {
        return Vec::new();
    };

    let mut candidates = names
        .into_iter()
        .filter(|name| is_sni_service_candidate(name))
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.dedup();
    candidates
}

fn is_sni_service_candidate(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    normalized.contains("statusnotifieritem")
        || normalized.contains("notificationitem")
        || is_qbittorrent_or_steam_service(name)
}

fn is_qbittorrent_or_steam_service(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    normalized.contains("qbittorrent") || normalized.contains("steam")
}

fn known_interface_for_reference<'a>(
    items: &'a HashMap<String, TrayItem>,
    reference: &str,
) -> Option<&'a str> {
    let (service, explicit_path) = normalize_item_reference(reference);
    items.values().find_map(|item| {
        if item.service_name != service {
            return None;
        }
        if let Some(path) = explicit_path.as_deref() {
            if item.object_path != path {
                return None;
            }
        }
        Some(item.interface_name.as_str())
    })
}

fn find_existing_item_for_reference<'a>(
    items: &'a HashMap<String, TrayItem>,
    reference: &str,
) -> Option<&'a TrayItem> {
    let (service, explicit_path) = normalize_item_reference(reference);
    items.values().find(|item| {
        item.service_name == service
            && explicit_path
                .as_deref()
                .map(|path| item.object_path == path)
                .unwrap_or(true)
    })
}

fn tray_items_differ(old: &HashMap<String, TrayItem>, new: &HashMap<String, TrayItem>) -> bool {
    if old.len() != new.len() {
        return true;
    }

    new.iter().any(|(key, next)| {
        let Some(prev) = old.get(key) else {
            return true;
        };

        prev.service_name != next.service_name
            || prev.object_path != next.object_path
            || prev.interface_name != next.interface_name
            || prev.id != next.id
            || prev.title != next.title
            || prev.category != next.category
            || prev.status != next.status
            || prev.icon_name != next.icon_name
            || prev.icon_theme_path != next.icon_theme_path
            || prev.tooltip != next.tooltip
            || prev.item_is_menu != next.item_is_menu
            || prev.menu_path != next.menu_path
            || prev
                .icon
                .as_ref()
                .map(|i| (i.width, i.height, i.rgba.as_slice()))
                != next
                    .icon
                    .as_ref()
                    .map(|i| (i.width, i.height, i.rgba.as_slice()))
    })
}

async fn load_tray_item(
    connection: &Connection,
    reference: &str,
    preferred_interface: Option<&str>,
) -> Option<TrayItem> {
    let (service_name, explicit_path) = normalize_item_reference(reference);

    // Probe the known interface first. This is important for idle CPU usage:
    // KDE SNI clients otherwise incur a failing freedesktop GetAll/Get sequence
    // before every successful KDE query.
    let interfaces: Vec<&str> = match preferred_interface {
        Some(ITEM_KDE_IFACE) => vec![ITEM_KDE_IFACE, ITEM_IFACE],
        Some(ITEM_IFACE) => vec![ITEM_IFACE, ITEM_KDE_IFACE],
        _ if service_name
            .to_ascii_lowercase()
            .contains("statusnotifieritem") =>
        {
            vec![ITEM_KDE_IFACE, ITEM_IFACE]
        }
        _ => vec![ITEM_IFACE, ITEM_KDE_IFACE],
    };

    let mut candidate_paths = candidate_item_paths(&service_name, explicit_path.as_deref());
    if explicit_path.is_none() {
        let discovered = discover_sni_object_paths(connection, &service_name).await;
        for path in discovered {
            if !candidate_paths.iter().any(|existing| existing == &path) {
                candidate_paths.push(path);
            }
        }
    }

    for object_path in candidate_paths {
        for interface_name in interfaces.iter().copied() {
            let props_proxy = match Proxy::new(
                connection,
                service_name.clone(),
                object_path.as_str(),
                PROPERTIES_IFACE,
            )
            .await
            {
                Ok(proxy) => proxy,
                Err(_) => continue,
            };

            let props: HashMap<String, OwnedValue> =
                match props_proxy.call("GetAll", &(interface_name,)).await {
                    Ok(value) => value,
                    Err(get_all_error) => {
                        // Some legacy SNI implementations (notably Steam/Ayatana)
                        // expose the interface but return an error for the aggregate
                        // GetAll call. Fetch the useful properties one by one instead
                        // of discarding the entire tray item.
                        let mut fallback = HashMap::new();
                        for property in [
                            "Id",
                            "Title",
                            "Category",
                            "Status",
                            "IconName",
                            "AttentionIconName",
                            "OverlayIconName",
                            "IconThemePath",
                            "ItemIsMenu",
                            "Menu",
                            "IconPixmap",
                            "AttentionIconPixmap",
                            "OverlayIconPixmap",
                        ] {
                            if let Ok(value) =
                                props_proxy.call("Get", &(interface_name, property)).await
                            {
                                fallback.insert(property.to_string(), value);
                            }
                        }
                        if fallback.is_empty() {
                            continue;
                        }
                        fallback
                    }
                };

            let id = value_string(&props, "Id").unwrap_or_default();
            let title = value_string(&props, "Title").unwrap_or_default();
            let category = value_string(&props, "Category").unwrap_or_default();
            let status = TrayStatus::from_dbus(&value_string(&props, "Status").unwrap_or_default());

            let icon_name = value_string(&props, "IconName").unwrap_or_default();
            let attention_icon_name = value_string(&props, "AttentionIconName").unwrap_or_default();
            let overlay_icon_name = value_string(&props, "OverlayIconName").unwrap_or_default();
            let icon_theme_path = value_string(&props, "IconThemePath").filter(|s| !s.is_empty());
            let tooltip = tooltip_text(&props);
            let item_is_menu = value_bool(&props, "ItemIsMenu").unwrap_or(false);
            let menu_path = value_object_path(&props, "Menu");

            let icon = resolve_icon_by_name(&icon_name, icon_theme_path.as_deref())
                .or_else(|| {
                    value_pixmaps(&props, "IconPixmap")
                        .as_deref()
                        .and_then(select_pixmap)
                })
                .or_else(|| resolve_icon_by_name(&attention_icon_name, icon_theme_path.as_deref()))
                .or_else(|| {
                    value_pixmaps(&props, "AttentionIconPixmap")
                        .as_deref()
                        .and_then(select_pixmap)
                })
                .or_else(|| resolve_icon_by_name(&overlay_icon_name, icon_theme_path.as_deref()))
                .or_else(|| resolve_icon_by_name(&id, icon_theme_path.as_deref()))
                .or_else(|| {
                    // Steam often registers a valid SNI item but provides no
                    // usable IconName/IconPixmap. Reuse the desktop icon resolver
                    // as a final visual fallback without affecting registration.
                    let steam_like = service_name.to_ascii_lowercase().contains("steam")
                        || id.to_ascii_lowercase().contains("steam")
                        || title.to_ascii_lowercase().contains("steam")
                        || object_path.to_ascii_lowercase().contains("steam");
                    if steam_like {
                        resolve_icon_by_name("steam", None)
                            .or_else(|| resolve_icon_by_name("steam_tray", None))
                    } else {
                        let qbittorrent_like =
                            service_name.to_ascii_lowercase().contains("qbittorrent")
                                || id.to_ascii_lowercase().contains("qbittorrent")
                                || title.to_ascii_lowercase().contains("qbittorrent")
                                || object_path.to_ascii_lowercase().contains("qbittorrent");
                        if qbittorrent_like {
                            resolve_icon_by_name("qbittorrent", None)
                                .or_else(|| resolve_icon_by_name("qbittorrent-tray", None))
                        } else {
                            None
                        }
                    }
                });

            let key = format!("{}{}", service_name, object_path);
            return Some(TrayItem {
                key,
                service_name,
                object_path,
                interface_name: interface_name.to_string(),
                id,
                title,
                category,
                status,
                icon_name,
                icon_theme_path,
                icon,
                tooltip,
                item_is_menu,
                menu_path,
            });
        }
    }

    None
}

async fn perform_action(connection: &Connection, action: &TrayAction) {
    let interfaces = if action.interface_name == ITEM_KDE_IFACE {
        [ITEM_KDE_IFACE, ITEM_IFACE]
    } else {
        [ITEM_IFACE, ITEM_KDE_IFACE]
    };

    for interface_name in interfaces {
        let proxy = match Proxy::new(
            connection,
            action.service_name.as_str(),
            action.object_path.as_str(),
            interface_name,
        )
        .await
        {
            Ok(proxy) => proxy,
            Err(_) => continue,
        };

        let result = match &action.kind {
            // Await the method reply. call_noreply only confirms that a message
            // was queued on the connection, which made the old "sent" log look
            // like a successful activation even when the item rejected it.
            TrayActionKind::Activate => {
                proxy
                    .call::<_, _, ()>("Activate", &(action.x, action.y))
                    .await
            }
            TrayActionKind::SecondaryActivate => {
                proxy
                    .call::<_, _, ()>("SecondaryActivate", &(action.x, action.y))
                    .await
            }
            TrayActionKind::ContextMenu => {
                if let Some(menu_path) = action.menu_path.as_deref() {
                    if let Ok(menu_proxy) = Proxy::new(
                        connection,
                        action.service_name.as_str(),
                        menu_path,
                        "com.canonical.dbusmenu",
                    )
                    .await
                    {
                        // AboutToShow lets menu providers refresh their action
                        // tree before the SNI ContextMenu request is dispatched.
                        let _ = menu_proxy.call::<_, _, bool>("AboutToShow", &(0i32,)).await;
                    }
                }
                proxy
                    .call::<_, _, ()>("ContextMenu", &(action.x, action.y))
                    .await
            }
            TrayActionKind::Scroll { delta, orientation } => {
                proxy
                    .call_noreply("Scroll", &(*delta, orientation.as_str()))
                    .await
            }
        };

        match result {
            Ok(()) => {
                return;
            }
            Err(_e) => {
            }
        }
    }
}

fn normalize_item_reference(reference: &str) -> (String, Option<String>) {
    let reference = reference.trim();
    if reference.is_empty() {
        return (String::new(), None);
    }

    // Local watcher references may be stored as `service/path`, where the
    // object path itself is represented without a leading slash after the
    // split. Normalize that form back to `(service, "/path")`.
    if let Some((service, path)) = reference.split_once('/') {
        if !service.is_empty() && !path.is_empty() {
            return (service.to_string(), Some(format!("/{path}")));
        }
    }

    (reference.to_string(), None)
}

/// Discover object paths exported by an SNI process when a watcher only
/// reported a service name. This covers older Ayatana/Steam layouts whose item
/// is not exported at `/StatusNotifierItem`.
async fn discover_sni_object_paths(connection: &Connection, service_name: &str) -> Vec<String> {
    let mut discovered = Vec::new();
    let mut queue = vec![(String::from("/"), 0usize)];
    let mut visited = HashSet::new();
    while let Some((path, depth)) = queue.pop() {
        if depth > 6 || visited.len() >= 32 || !visited.insert(path.clone()) {
            continue;
        }
        let Ok(proxy) = Proxy::new(
            connection,
            service_name,
            path.as_str(),
            "org.freedesktop.DBus.Introspectable",
        )
        .await
        else {
            continue;
        };
        let Ok(xml) = proxy.call::<_, _, String>("Introspect", &()).await else {
            continue;
        };
        if xml.contains(ITEM_IFACE) || xml.contains(ITEM_KDE_IFACE) {
            if path != "/" {
                discovered.push(path.clone());
            }
        }
        for child in introspection_child_nodes(&xml) {
            let child_path = if child.starts_with('/') {
                child
            } else if path == "/" {
                format!("/{child}")
            } else {
                format!("{}/{child}", path.trim_end_matches('/'))
            };
            if child_path.contains("StatusNotifierItem")
                || child_path.contains("NotificationItem")
                || child_path.to_ascii_lowercase().contains("steam")
            {
                discovered.push(child_path.clone());
            }
            queue.push((child_path, depth + 1));
        }
    }
    discovered.sort();
    discovered.dedup();
    if std::env::var_os("DOCK_TRAY_DEBUG").is_some() && !discovered.is_empty() {
    }
    discovered
}

/// Extract child `<node name="..."/>` entries from standard D-Bus
/// introspection XML without pulling a second XML parser into the hot path.
fn introspection_child_nodes(xml: &str) -> Vec<String> {
    let mut nodes = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<node") {
        rest = &rest[start + 5..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..end];
        rest = &rest[end + 1..];
        let Some(name_pos) = tag.find("name") else {
            continue;
        };
        let after_name = &tag[name_pos + 4..];
        let Some(eq_pos) = after_name.find('=') else {
            continue;
        };
        let value = after_name[eq_pos + 1..].trim_start();
        let Some(quote) = value.chars().next().filter(|c| *c == '\'' || *c == '"') else {
            continue;
        };
        let value = &value[quote.len_utf8()..];
        let Some(value_end) = value.find(quote) else {
            continue;
        };
        let name = value[..value_end].trim();
        if !name.is_empty() && !name.contains("..") {
            nodes.push(name.to_string());
        }
    }
    nodes
}

fn candidate_item_paths(service_name: &str, explicit_path: Option<&str>) -> Vec<String> {
    let sanitized = service_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();

    let mut paths = Vec::with_capacity(8);
    if let Some(path) = explicit_path.filter(|p| !p.is_empty()) {
        paths.push(path.to_string());
    }
    paths.push("/StatusNotifierItem".to_string());
    paths.push("/org/kde/StatusNotifierItem".to_string());
    paths.push("/org/ayatana/NotificationItem/StatusNotifierItem".to_string());
    paths.push(format!("/org/ayatana/NotificationItem/{sanitized}"));
    let lower = service_name.to_ascii_lowercase();
    if lower.contains("steam") {
        paths.push("/org/ayatana/NotificationItem/steam".to_string());
    }
    if lower.contains("qbittorrent") {
        paths.push("/org/ayatana/NotificationItem/qbittorrent".to_string());
        paths.push("/org/ayatana/NotificationItem/QBittorrent".to_string());
    }
    paths.push("/".to_string());
    let mut seen = HashSet::new();
    paths.retain(|path| seen.insert(path.clone()));
    paths
}

fn value_string(props: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    props
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| String::try_from(value).ok())
}

fn value_bool(props: &HashMap<String, OwnedValue>, key: &str) -> Option<bool> {
    props
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| bool::try_from(value).ok())
}

fn value_object_path(props: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    props
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| OwnedObjectPath::try_from(value).ok())
        .map(|path| path.to_string())
        .filter(|path| path != "/")
}

fn value_pixmaps(
    props: &HashMap<String, OwnedValue>,
    key: &str,
) -> Option<Vec<(i32, i32, Vec<u8>)>> {
    props
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Vec::<(i32, i32, Vec<u8>)>::try_from(value).ok())
}

fn select_pixmap(pixmaps: &[(i32, i32, Vec<u8>)]) -> Option<TrayIcon> {
    let best = pixmaps
        .iter()
        .filter(|(w, h, data)| {
            *w > 0
                && *h > 0
                && data.len() >= (*w as usize).saturating_mul(*h as usize).saturating_mul(4)
                && data
                    .chunks_exact(4)
                    .take((*w as usize).saturating_mul(*h as usize))
                    .any(|pixel| pixel[0] != 0)
        })
        .min_by_key(|(w, h, _)| {
            let target = 32i32;
            (w - target).abs() + (h - target).abs()
        })?;

    let (width, height, data) = best;
    let pixel_count = (*width as usize).saturating_mul(*height as usize);
    let mut rgba = Vec::with_capacity(pixel_count * 4);

    for chunk in data.chunks_exact(4).take(pixel_count) {
        // SNI transmits ARGB32 in network byte order: A,R,G,B.
        rgba.extend_from_slice(&[chunk[1], chunk[2], chunk[3], chunk[0]]);
    }

    Some(TrayIcon {
        rgba,
        width: *width as u32,
        height: *height as u32,
    })
}

fn resolve_icon_by_name(icon_name: &str, theme_path: Option<&str>) -> Option<TrayIcon> {
    if icon_name.is_empty() {
        return None;
    }

    let mut candidates = Vec::<PathBuf>::new();
    let path = Path::new(icon_name);
    if path.is_absolute() && path.exists() {
        candidates.push(path.to_path_buf());
    }

    if let Some(theme_root) = theme_path.filter(|p| !p.is_empty()) {
        let root = Path::new(theme_root);
        for ext in ["png", "svg", "xpm", "jpg"] {
            candidates.push(root.join(format!("{icon_name}.{ext}")));
            candidates.push(
                root.join("scalable")
                    .join("apps")
                    .join(format!("{icon_name}.{ext}")),
            );
            candidates.push(
                root.join("48x48")
                    .join("apps")
                    .join(format!("{icon_name}.{ext}")),
            );
            candidates.push(
                root.join("32x32")
                    .join("apps")
                    .join(format!("{icon_name}.{ext}")),
            );
        }
    }

    if let Some(path) = crate::resolvers::get_icon_path(icon_name) {
        candidates.push(path);
    }

    for root in ["/usr/share/icons", "/usr/local/share/icons"] {
        for theme in ["hicolor", "Adwaita", "Papirus", "breeze", "default"] {
            for size in [
                "scalable/apps",
                "256x256/apps",
                "128x128/apps",
                "64x64/apps",
                "48x48/apps",
                "32x32/apps",
                "24x24/apps",
                "22x22/apps",
                "16x16/apps",
            ] {
                for ext in ["png", "svg"] {
                    candidates.push(
                        Path::new(root)
                            .join(theme)
                            .join(size)
                            .join(format!("{icon_name}.{ext}")),
                    );
                }
            }
        }
    }
    for root in ["/usr/share/pixmaps", "/usr/local/share/pixmaps"] {
        for ext in ["png", "svg", "xpm"] {
            candidates.push(Path::new(root).join(format!("{icon_name}.{ext}")));
        }
    }

    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }
        if let Some((w, h, rgba)) = crate::terminal_graphics::load_image_raw_rgba(&candidate, 32) {
            return Some(TrayIcon {
                rgba,
                width: w,
                height: h,
            });
        }
    }

    None
}

fn tooltip_text(props: &HashMap<String, OwnedValue>) -> String {
    // Tooltip is a four-field SNI structure. Keep only the textual title and
    // description; the icon payload is unnecessary for the mini-dock.
    props
        .get("ToolTip")
        .and_then(|value| {
            value
                .try_clone()
                .ok()
                .and_then(|value| {
                    <(String, Vec<(i32, i32, Vec<u8>)>, String, String)>::try_from(value).ok()
                })
                .map(|(_, _, title, description)| {
                    if description.is_empty() {
                        title
                    } else if title.is_empty() {
                        description
                    } else {
                        format!("{title}: {description}")
                    }
                })
        })
        .unwrap_or_default()
}

fn publish_snapshot(
    tx: &tokio::sync::mpsc::UnboundedSender<TraySnapshot>,
    items: &HashMap<String, TrayItem>,
) {
    let mut snapshot = items.values().cloned().collect::<Vec<_>>();
    snapshot.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.service_name.cmp(&b.service_name))
    });
    let _ = tx.send(TraySnapshot { items: snapshot });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_and_default_item_paths_are_ordered() {
        let paths = candidate_item_paths(":1.42", Some("/org/ayatana/NotificationItem/steam"));
        assert_eq!(paths[0], "/org/ayatana/NotificationItem/steam");
        assert_eq!(paths[1], "/StatusNotifierItem");
    }

    #[test]
    fn known_qbittorrent_and_sni_bus_names_are_fallback_candidates() {
        assert!(is_sni_service_candidate(
            "org.kde.StatusNotifierItem-1234-1"
        ));
        assert!(is_sni_service_candidate("org.qbittorrent.qBittorrent"));
        assert!(!is_sni_service_candidate("org.freedesktop.Notifications"));
    }

    #[test]
    fn qbittorrent_paths_are_candidates_for_qt_services() {
        let paths = candidate_item_paths("org.qbittorrent.qBittorrent", None);
        assert!(paths.iter().any(|path| path == "/StatusNotifierItem"));
        assert!(paths
            .iter()
            .any(|path| path == "/org/ayatana/NotificationItem/qbittorrent"));
    }

    #[test]
    fn fully_transparent_pixmaps_are_rejected() {
        let transparent = vec![0u8; 22 * 22 * 4];
        assert!(select_pixmap(&[(22, 22, transparent)]).is_none());
    }

    #[test]
    fn normalize_service_and_object_path_reference() {
        let (service, path) = normalize_item_reference(":1.49/org/ayatana/NotificationItem/steam");
        assert_eq!(service, ":1.49");
        assert_eq!(path.as_deref(), Some("/org/ayatana/NotificationItem/steam"));
    }

    #[test]
    fn normalize_service_only_reference() {
        let (service, path) = normalize_item_reference(":1.700");
        assert_eq!(service, ":1.700");
        assert!(path.is_none());
    }

    #[test]
    fn introspection_parser_finds_child_object_paths() {
        let xml = r#"<node><node name="org"/><node name='StatusNotifierItem'/></node>"#;
        let mut nodes = introspection_child_nodes(xml);
        nodes.sort();
        assert_eq!(
            nodes,
            vec!["StatusNotifierItem".to_string(), "org".to_string()]
        );
    }

    #[test]
    fn steam_known_path_is_a_candidate_for_named_services() {
        let paths = candidate_item_paths("org.example.steam", None);
        assert!(paths
            .iter()
            .any(|path| path == "/org/ayatana/NotificationItem/steam"));
    }
}
