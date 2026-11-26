/* SPDX-License-Identifier: MPL-2.0 */
use accesskit::{
    Action, ActionData, ActionRequest, Live, Node, NodeId, Rect, Role, TextPosition, TextSelection,
    Toggled, TreeId, TreeInfo, TreeUpdate,
};
#[cfg(any(windows, target_os = "linux"))]
use accesskit::{ActionHandler, ActivationHandler};
use gpui::{
    AnyElement, App, Bounds, Context, Element, ElementId, Entity, EntityInputHandler, FocusHandle,
    Focusable, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Render, Window,
    px,
};
use gpui_component::input::InputState;
use std::{
    cell::RefCell,
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    rc::Rc,
    sync::{Arc, Mutex},
};

const ROOT: NodeId = NodeId(0);
type Snapshot = Arc<Mutex<Option<TreeUpdate>>>;
type Invoke = Rc<dyn Fn(ActionRequest, &mut Window, &mut App)>;

#[cfg(any(windows, target_os = "linux"))]
struct Activation(Snapshot);
#[cfg(any(windows, target_os = "linux"))]
impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.0.lock().ok()?.clone()
    }
}

#[cfg(any(windows, target_os = "linux"))]
struct Actions(async_channel::Sender<ActionRequest>);
#[cfg(any(windows, target_os = "linux"))]
impl ActionHandler for Actions {
    fn do_action(&mut self, request: ActionRequest) {
        // Platform callbacks may run off the GPUI thread. Queue a bounded request and return.
        let _ = self.0.try_send(request);
    }
}

#[cfg(target_os = "linux")]
struct Deactivation;
#[cfg(target_os = "linux")]
impl accesskit::DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {}
}

#[cfg(any(windows, target_os = "linux"))]
enum Platform {
    #[cfg(windows)]
    Windows(accesskit_windows::SubclassingAdapter),
    #[cfg(target_os = "linux")]
    Linux(accesskit_unix::Adapter),
}

#[cfg(any(windows, target_os = "linux"))]
impl Platform {
    #[cfg(windows)]
    fn attach(window: &Window, activation: Activation, actions: Actions) -> Self {
        use accesskit_windows::{HWND, SubclassingAdapter};
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let handle =
            HasWindowHandle::window_handle(window).expect("GPUI Win32 window has a native handle");
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            unreachable!("GPUI Windows window has a Win32 handle");
        };
        Self::Windows(SubclassingAdapter::new(
            HWND(handle.hwnd.get() as *mut _),
            activation,
            actions,
        ))
    }

    #[cfg(target_os = "linux")]
    fn attach(_: &Window, activation: Activation, actions: Actions) -> Self {
        Self::Linux(accesskit_unix::Adapter::new(
            activation,
            actions,
            Deactivation,
        ))
    }
}

#[derive(Default)]
struct Frame {
    nodes: Vec<(NodeId, Node)>,
    parents: Vec<NodeId>,
    children: HashMap<NodeId, Vec<NodeId>>,
    focus: Option<NodeId>,
    routes: HashMap<NodeId, Invoke>,
}

struct State {
    focus_after_paint: RefCell<Option<ElementId>>,
    title: String,
    pinned_snapshot: Snapshot,
    #[cfg(any(windows, target_os = "linux"))]
    platform: RefCell<Platform>,
    frame: RefCell<Frame>,
    routes: RefCell<HashMap<NodeId, Invoke>>,
}

/// One real adapter per GPUI top-level window. Render wrappers capture painted geometry,
/// focus and commands from those controls. The platform thread only sees immutable trees.
#[derive(Clone)]
pub struct Accessibility(Rc<State>);

impl Accessibility {
    /// Resolve a navigation destination against the newly mounted button.
    /// A handle from the old surface can point to a control that no longer exists.
    pub fn focus_button_after_paint(&self, id: impl Into<ElementId>) {
        self.0.focus_after_paint.replace(Some(id.into()));
    }

    pub fn new<T: Render>(window: &mut Window, cx: &mut Context<T>, title: String) -> Self {
        let snapshot = Snapshot::default();
        #[cfg(any(windows, target_os = "linux"))]
        let (send, receive) = async_channel::bounded(128);
        #[cfg(any(windows, target_os = "linux"))]
        let platform = Platform::attach(window, Activation(snapshot.clone()), Actions(send));
        #[cfg(not(any(windows, target_os = "linux")))]
        let _ = (window, cx);
        let state = Rc::new(State {
            focus_after_paint: RefCell::default(),
            title,
            pinned_snapshot: snapshot,
            #[cfg(any(windows, target_os = "linux"))]
            platform: RefCell::new(platform),
            frame: RefCell::new(Frame::default()),
            routes: RefCell::new(HashMap::new()),
        });
        #[cfg(any(windows, target_os = "linux"))]
        let weak = Rc::downgrade(&state);
        #[cfg(any(windows, target_os = "linux"))]
        cx.spawn_in(window, async move |_, cx| {
            while let Ok(request) = receive.recv().await {
                #[cfg(target_os = "linux")]
                if request.target_tree != TreeId::ROOT
                    || !matches!(
                        request.action,
                        Action::Focus | Action::Click | Action::SetValue
                    )
                {
                    continue;
                }
                let Some(state) = weak.upgrade() else { break };
                if cx
                    .update(move |window, cx| {
                        let route = state.routes.borrow().get(&request.target_node).cloned();
                        if let Some(route) = route {
                            route(request, window, cx);
                            // X11 may remain idle after a bus-only action. Draw after the entity
                            // updates so AT can read the new focus and values immediately.
                            #[cfg(target_os = "linux")]
                            window.defer(cx, |window, cx| {
                                window.refresh();
                                window.draw(cx).clear();
                            });
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        #[cfg(target_os = "linux")]
        {
            let weak = Rc::downgrade(&state);
            cx.observe_window_activation(window, move |_, window, _| {
                if let Some(state) = weak.upgrade() {
                    let Platform::Linux(adapter) = &mut *state.platform.borrow_mut();
                    adapter.update_window_focus_state(window.is_window_active());
                }
            })
            .detach();
        }
        Self(state)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Keep visual identity, accessibility state, and activation on one control."
    )]
    pub fn button(
        &self,
        element: impl IntoElement,
        id: ElementId,
        label: impl Into<String>,
        role: Role,
        disabled: bool,
        selected: Option<bool>,
        action: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Semantic {
        let mut node = Node::new(role);
        node.set_label(label.into());
        if let Some(selected) = selected {
            node.set_selected(selected);
            if role == Role::RadioButton {
                node.set_toggled(Toggled::from(selected));
            }
        }
        if disabled {
            node.set_disabled();
        } else {
            node.add_action(Action::Focus);
            node.add_action(Action::Click);
        }
        let kind = if disabled {
            Binding::None
        } else {
            Binding::Button(
                id.clone(),
                Rc::new(move |request, window, cx| {
                    if request.action == Action::Click {
                        action(window, cx);
                    }
                }),
            )
        };
        self.wrap(element, format!("{id:?}"), node, kind)
    }

    pub fn input(
        &self,
        element: impl IntoElement,
        label: &'static str,
        role: Role,
        input: &Entity<InputState>,
        disabled: bool,
        cx: &App,
    ) -> Semantic {
        let mut node = Node::new(role);
        node.set_label(label);
        node.set_value(input.read(cx).value().to_string());
        let kind = if disabled {
            node.set_disabled();
            Binding::None
        } else {
            node.add_action(Action::Focus);
            node.add_action(Action::SetValue);
            let focus = input.read(cx).focus_handle(cx);
            let editable = input.downgrade();
            Binding::Input(
                input.clone(),
                focus,
                Rc::new(move |request, window, cx| {
                    if let (Action::SetValue, Some(ActionData::Value(value))) =
                        (request.action, request.data)
                        && value.len() <= 4096
                    {
                        let _ = editable.update(cx, |input, cx| {
                            input.set_value(value.into_string(), window, cx);
                        });
                    }
                }),
            )
        };
        self.wrap(element, label.into(), node, kind)
    }

    pub fn text(
        &self,
        element: impl IntoElement,
        id: impl Into<String>,
        text: impl Into<String>,
        role: Role,
    ) -> Semantic {
        let mut node = Node::new(role);
        if role == Role::Label {
            node.set_value(text.into());
        } else {
            node.set_label(text.into());
        }
        if matches!(role, Role::Status | Role::Alert) {
            node.set_live(if role == Role::Alert {
                Live::Assertive
            } else {
                Live::Polite
            });
        }
        self.wrap(element, id.into(), node, Binding::None)
    }

    pub fn progress(&self, meter: &Entity<super::progress::Meter>, cx: &App) -> Semantic {
        let mut node = Node::new(Role::ProgressIndicator);
        let update = meter.read(cx).target;
        node.set_label(update.label());
        if let super::progress::Update::Transfer(sample) = update {
            node.set_value(sample.counts());
            if let Some(fraction) = sample.fraction() {
                node.set_min_numeric_value(0.);
                node.set_max_numeric_value(100.);
                node.set_numeric_value(fraction * 100.);
            }
        }
        self.wrap(meter.clone(), "task-progress".into(), node, Binding::None)
    }

    pub fn group(
        &self,
        element: impl IntoElement,
        id: impl Into<String>,
        label: impl Into<String>,
        role: Role,
    ) -> Semantic {
        let mut node = Node::new(role);
        node.set_label(label.into());
        self.wrap(element, id.into(), node, Binding::Group(None))
    }

    pub fn focus_group(
        &self,
        element: impl IntoElement,
        id: &'static str,
        label: &'static str,
        role: Role,
        focus: &FocusHandle,
    ) -> Semantic {
        let mut node = Node::new(role);
        node.set_label(label);
        node.add_action(Action::Focus);
        self.wrap(
            element,
            id.into(),
            node,
            Binding::Group(Some(focus.clone())),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "A splitter reports its label, role, bounds, value, and focus together."
    )]
    pub fn splitter(
        &self,
        element: impl IntoElement,
        id: &'static str,
        label: &'static str,
        minimum: f32,
        maximum: f32,
        value: f32,
        focus: FocusHandle,
    ) -> Semantic {
        let mut node = Node::new(Role::Splitter);
        node.set_label(label);
        node.set_description(
            "Use arrow keys to resize. Home and End set the bounds. Enter resets.",
        );
        node.set_orientation(accesskit::Orientation::Vertical);
        node.set_min_numeric_value(f64::from(minimum));
        node.set_max_numeric_value(f64::from(maximum));
        node.set_numeric_value(f64::from(value));
        node.add_action(Action::Focus);
        self.wrap(element, id.into(), node, Binding::Group(Some(focus)))
    }

    pub fn frame(&self, element: impl IntoElement, blocked: bool) -> Semantic {
        self.wrap(
            element,
            String::new(),
            Node::new(Role::Window),
            Binding::Frame(blocked),
        )
    }

    fn wrap(
        &self,
        element: impl IntoElement,
        id: String,
        node: Node,
        binding: Binding,
    ) -> Semantic {
        Semantic {
            state: self.0.clone(),
            element: element.into_any_element(),
            id,
            node,
            binding,
            focus_outline: None,
        }
    }
}

enum Binding {
    None,
    Group(Option<FocusHandle>),
    Frame(bool),
    Button(ElementId, Invoke),
    TrackedControl(FocusHandle, Invoke),
    Input(Entity<InputState>, FocusHandle, Invoke),
}

pub struct Semantic {
    state: Rc<State>,
    element: AnyElement,
    id: String,
    node: Node,
    binding: Binding,
    focus_outline: Option<gpui::Hsla>,
}

impl Semantic {
    pub fn with_focus_outline(mut self, color: gpui::Hsla) -> Self {
        self.focus_outline = Some(color);
        self
    }

    pub fn with_focus(mut self, handle: FocusHandle) -> Self {
        if let Binding::Button(_, callback) = &self.binding {
            self.binding = Binding::TrackedControl(handle, callback.clone());
        }
        self
    }
}

impl IntoElement for Semantic {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Semantic {
    type RequestLayoutState = Option<FocusHandle>;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Option<FocusHandle>) {
        let layout = self.element.request_layout(window, cx);
        let focus = match &self.binding {
            Binding::Button(id, _) => {
                // GPUI Component pushes the Button type name before Button::render uses this key.
                let component =
                    ElementId::Name(std::any::type_name::<gpui_component::button::Button>().into());
                let handle = window.with_id(component, |window| {
                    window.use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle())
                });
                Some(handle.read(cx).clone())
            }
            Binding::Input(_, focus, _)
            | Binding::Group(Some(focus))
            | Binding::TrackedControl(focus, _) => Some(focus.clone()),
            _ => None,
        };
        (layout, focus)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Option<FocusHandle>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.element.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        focus: &mut Option<FocusHandle>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Binding::Frame(blocked) = self.binding {
            self.state.frame.replace(Frame::default());
            self.element.paint(window, cx);
            self.state.publish(bounds, blocked, window, cx);
            return;
        }

        let node_id = {
            let mut frame = self.state.frame.borrow_mut();
            let parent = frame.parents.last().copied().unwrap_or(ROOT);
            let id = node_id(parent, &self.id);
            let mut node = self.node.clone();
            let clipped = bounds.intersect(&window.content_mask().bounds);
            if clipped.size.width > px(0.) && clipped.size.height > px(0.) {
                node.set_bounds(rect(clipped, scale(window)));
            }
            if let Some(focus) = focus {
                let focus_requested = matches!(&self.binding, Binding::Button(button_id, _)
                    if self.state.focus_after_paint.borrow().as_ref() == Some(button_id));
                if focus_requested {
                    self.state.focus_after_paint.take();
                    let target = focus.clone();
                    window.on_next_frame(move |window, _| target.focus(window));
                }
                if focus.is_focused(window) {
                    frame.focus = Some(id);
                }
                let callback = match &self.binding {
                    Binding::Button(_, callback)
                    | Binding::Input(_, _, callback)
                    | Binding::TrackedControl(_, callback) => Some(callback.clone()),
                    _ => None,
                };
                let focus = focus.clone();
                frame.routes.insert(
                    id,
                    Rc::new(move |request, window, cx| {
                        if request.action == Action::Focus {
                            if !window.is_window_active() {
                                window.activate_window();
                            }
                            focus.focus(window);
                        } else if let Some(callback) = &callback {
                            callback(request, window, cx);
                        }
                    }),
                );
            }
            if matches!(node.role(), Role::TextInput | Role::SearchInput) {
                // AT-SPI EditableText is available only when the field has a text run.
                let text = node.value().unwrap_or_default();
                let mut run = Node::new(Role::TextRun);
                run.set_value(text);
                run.set_character_lengths(
                    text.chars()
                        .map(|ch| ch.len_utf8() as u8)
                        .collect::<Vec<_>>(),
                );
                let run_id = node_id(id, "text");
                if let Binding::Input(input, _, _) = &self.binding {
                    let selection =
                        input.update(cx, |input, cx| input.selected_text_range(false, window, cx));
                    if let Some(selection) = selection {
                        let position = |offset| TextPosition {
                            node: run_id,
                            character_index: scalar_index(text, offset),
                        };
                        let (start, end) = (
                            position(selection.range.start),
                            position(selection.range.end),
                        );
                        let (anchor, focus) = if selection.reversed {
                            (end, start)
                        } else {
                            (start, end)
                        };
                        node.set_text_selection(TextSelection { anchor, focus });
                    }
                }
                frame.children.entry(id).or_default().push(run_id);
                frame.nodes.push((run_id, run));
            }
            frame.children.entry(parent).or_default().push(id);
            frame.nodes.push((id, node));
            if matches!(self.binding, Binding::Group(_)) {
                frame.parents.push(id);
            }
            id
        };

        self.element.paint(window, cx);
        if let Some(color) = self.focus_outline
            && focus
                .as_ref()
                .is_some_and(|handle| handle.is_focused(window))
        {
            let mut ring = gpui::outline(bounds.dilate(px(4.)), color, gpui::BorderStyle::Solid);
            ring.border_widths = px(2.).into();
            window.paint_quad(ring);
        }
        if matches!(self.binding, Binding::Group(_)) {
            let parent = self.state.frame.borrow_mut().parents.pop();
            debug_assert_eq!(parent, Some(node_id));
        }
    }
}

impl State {
    fn publish(
        self: &Rc<Self>,
        bounds: Bounds<Pixels>,
        blocked: bool,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let mut frame = self.frame.take();
        self.focus_after_paint.take();
        for (id, node) in &mut frame.nodes {
            if let Some(children) = frame.children.remove(id) {
                node.set_children(children);
            }
            if blocked && frame.routes.contains_key(id) {
                node.set_disabled();
                node.remove_action(Action::Focus);
                node.remove_action(Action::Click);
                node.remove_action(Action::SetValue);
            }
        }
        let mut root = Node::new(Role::Window);
        root.set_label(self.title.clone());
        root.set_bounds(rect(bounds, scale(window)));
        root.set_children(frame.children.remove(&ROOT).unwrap_or_default());
        frame.nodes.push((ROOT, root));
        let mut tree = TreeInfo::new(ROOT);
        tree.toolkit_name = Some("GPUI".into());
        let update = TreeUpdate {
            nodes: frame.nodes,
            tree: Some(tree),
            tree_id: TreeId::ROOT,
            focus: if blocked {
                ROOT
            } else {
                frame.focus.unwrap_or(ROOT)
            },
        };
        self.routes.replace(if blocked {
            HashMap::new()
        } else {
            frame.routes
        });

        let unchanged = self
            .pinned_snapshot
            .lock()
            .is_ok_and(|seen| seen.as_ref() == Some(&update));
        if !unchanged && let Ok(mut snapshot) = self.pinned_snapshot.lock() {
            snapshot.replace(update.clone());
        }
        #[cfg(any(windows, target_os = "linux"))]
        let state = self.clone();
        #[cfg(any(windows, target_os = "linux"))]
        window.defer(_cx, move |_window, _| {
            #[cfg(windows)]
            if !unchanged {
                let events = {
                    let Platform::Windows(adapter) = &mut *state.platform.borrow_mut();
                    adapter.update_if_active(|| update)
                };
                // UIA may synchronously make a new WM_GETOBJECT request.
                if let Some(events) = events {
                    events.raise();
                }
            }
            #[cfg(target_os = "linux")]
            {
                let Platform::Linux(adapter) = &mut *state.platform.borrow_mut();
                // GPUI coordinates are logical pixels. Wayland cannot report a global
                // origin. X11 bounds come from GPUI's current platform window.
                let outer = rect(_window.bounds(), 1.);
                let inner = Rect::new(
                    outer.x0,
                    outer.y0,
                    outer.x0 + f64::from(bounds.size.width),
                    outer.y0 + f64::from(bounds.size.height),
                );
                adapter.set_root_window_bounds(outer, inner);
                adapter.update_window_focus_state(_window.is_window_active());
                if !unchanged {
                    adapter.update_if_active(|| update);
                }
            }
        });
    }
}

fn node_id(parent: NodeId, id: &str) -> NodeId {
    let mut hash = DefaultHasher::new();
    parent.hash(&mut hash);
    id.hash(&mut hash);
    NodeId(hash.finish().max(1))
}

fn scalar_index(text: &str, utf16_offset: usize) -> usize {
    let mut units = 0;
    text.chars()
        .take_while(|ch| {
            units += ch.len_utf16();
            units <= utf16_offset
        })
        .count()
}

fn scale(window: &Window) -> f64 {
    if cfg!(windows) {
        f64::from(window.scale_factor())
    } else {
        1.
    }
}

fn rect(bounds: Bounds<Pixels>, scale: f64) -> Rect {
    let end = bounds.bottom_right();
    Rect::new(
        f64::from(bounds.origin.x) * scale,
        f64::from(bounds.origin.y) * scale,
        f64::from(end.x) * scale,
        f64::from(end.y) * scale,
    )
}
