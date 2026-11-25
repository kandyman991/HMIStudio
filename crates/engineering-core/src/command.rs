//! Typed, serializable engineering commands.
//!
//! Commands are plain data. The UI, tests and future AI agents all build the
//! same `Command` values and submit them through
//! [`crate::ProjectSession::execute`]. There is no generic "set any JSON
//! path" command and no shell or filesystem command.

use std::collections::BTreeMap;

use hmi_project_model::model::*;
use serde::{Deserialize, Serialize};

/// One engineering operation. JSON form: `{ "op": "<name>", ... }`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", deny_unknown_fields)]
pub enum Command {
    // ---------------------------------------------------------- screens
    /// Creates a screen. Without `root`, an empty full-size column is created.
    /// Without `id`, one is allocated (`scr-N`).
    #[serde(rename_all = "camelCase")]
    CreateScreen {
        /// Explicit id, or allocate.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<ScreenId>,
        /// Engineer-facing name.
        name: String,
        /// Page or popup.
        #[serde(default)]
        kind: ScreenKind,
        /// Root node (ids kept as given).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        root: Option<Node>,
        /// Insert position in `screens` (default: end).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    /// Renames a screen (its id never changes).
    #[serde(rename_all = "camelCase")]
    RenameScreen {
        /// Screen.
        screen: ScreenId,
        /// New name.
        name: String,
    },
    /// Sets or clears the operator-facing title.
    #[serde(rename_all = "camelCase")]
    SetScreenTitle {
        /// Screen.
        screen: ScreenId,
        /// Title.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<LocalizedText>,
    },
    /// Deletes a screen. Fails validation if it is still referenced.
    #[serde(rename_all = "camelCase")]
    DeleteScreen {
        /// Screen.
        screen: ScreenId,
    },
    /// Duplicates a screen with new ids for the screen and every node;
    /// actions inside the copy that targeted the original screen are
    /// redirected to the copy.
    #[serde(rename_all = "camelCase")]
    DuplicateScreen {
        /// Source screen.
        screen: ScreenId,
        /// Name of the copy (default: `<name> copy`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },

    // ------------------------------------------------------------ nodes
    /// Inserts a node subtree under `parent`.
    #[serde(rename_all = "camelCase")]
    InsertNode {
        /// Parent node.
        parent: NodeId,
        /// Child index (default: end).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
        /// Node subtree.
        node: Node,
        /// Replace every id in the subtree with freshly allocated ones.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        assign_new_ids: bool,
    },
    /// Insert a new primitive/input together with its typed contract.
    #[serde(rename_all = "camelCase")]
    InsertV2Component {
        parent: NodeId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<NodeId>,
        #[serde(default)]
        layout: LayoutParams,
        contract: hmi_project_model::model::v2_components::ComponentContract,
    },
    /// Replace only one typed component contract, never the whole v2 group.
    #[serde(rename_all = "camelCase")]
    SetV2ComponentContract {
        node: NodeId,
        contract: hmi_project_model::model::v2_components::ComponentContract,
    },
    /// Removes a node and its subtree. Roots cannot be removed.
    #[serde(rename_all = "camelCase")]
    RemoveNode {
        /// Node.
        node: NodeId,
    },
    /// Copies a subtree with new ids, by default right after the original.
    #[serde(rename_all = "camelCase")]
    DuplicateNode {
        /// Node to copy.
        node: NodeId,
        /// Target parent (default: the original's parent).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<NodeId>,
        /// Target index (default: after the original / end).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    /// Moves (reparents and/or reorders) a node. `index` is the position in
    /// the target parent's children *after* removal from the old position.
    #[serde(rename_all = "camelCase")]
    MoveNode {
        /// Node.
        node: NodeId,
        /// New parent.
        parent: NodeId,
        /// New index (default: end).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    /// Reorders a node within its current parent.
    #[serde(rename_all = "camelCase")]
    ReorderNode {
        /// Node.
        node: NodeId,
        /// New index among its siblings.
        index: usize,
    },
    /// Sets the designer layer name.
    #[serde(rename_all = "camelCase")]
    RenameNode {
        /// Node.
        node: NodeId,
        /// Name (unset clears it).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    /// Replaces the element description (e.g. row spacing, grid tracks).
    #[serde(rename_all = "camelCase")]
    SetElement {
        /// Node.
        node: NodeId,
        /// New element.
        element: Element,
    },
    /// Sets (`value`) or removes (no `value`) one property.
    #[serde(rename_all = "camelCase")]
    SetProperty {
        /// Node.
        node: NodeId,
        /// Property name.
        name: String,
        /// Value; absent removes the property.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<serde_json::Value>,
    },
    /// Assigns or clears a named style.
    #[serde(rename_all = "camelCase")]
    SetNodeStyle {
        /// Node.
        node: NodeId,
        /// Style; absent clears it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<StyleId>,
    },
    /// Replaces the node's layout parameters.
    #[serde(rename_all = "camelCase")]
    SetLayout {
        /// Node.
        node: NodeId,
        /// Layout parameters.
        layout: LayoutParams,
    },
    /// Replaces the node's responsive rules.
    #[serde(rename_all = "camelCase")]
    SetResponsive {
        /// Node.
        node: NodeId,
        /// Rules.
        rules: Vec<ResponsiveRule>,
    },
    /// Adds or replaces (same `property`) a binding.
    #[serde(rename_all = "camelCase")]
    SetBinding {
        /// Node.
        node: NodeId,
        /// Binding.
        binding: PropertyBinding,
    },
    /// Removes the binding for `property`.
    #[serde(rename_all = "camelCase")]
    RemoveBinding {
        /// Node.
        node: NodeId,
        /// Bound property.
        property: String,
    },
    /// Adds or replaces (same `event`) an event handler.
    #[serde(rename_all = "camelCase")]
    SetEventHandler {
        /// Node.
        node: NodeId,
        /// Handler.
        handler: EventHandler,
    },
    /// Removes the handler for `event`.
    #[serde(rename_all = "camelCase")]
    RemoveEventHandler {
        /// Node.
        node: NodeId,
        /// Event.
        event: EventKind,
    },

    // ------------------------------------------------------------- tags
    /// Creates a tag.
    #[serde(rename_all = "camelCase")]
    CreateTag {
        /// Tag (its id must be new).
        tag: Tag,
        /// Position (default: end).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
    },
    /// Replaces an existing tag with the same id.
    #[serde(rename_all = "camelCase")]
    UpdateTag {
        /// New tag value.
        tag: Tag,
    },
    /// Deletes a tag. Fails validation if it is still referenced.
    #[serde(rename_all = "camelCase")]
    DeleteTag {
        /// Tag.
        tag: TagId,
    },

    // ----------------------------------------------------------- styles
    /// Adds or replaces (same id) a named style.
    #[serde(rename_all = "camelCase")]
    UpsertStyle {
        /// Style.
        style: Style,
    },
    /// Deletes a style. Fails validation if it is still referenced.
    #[serde(rename_all = "camelCase")]
    DeleteStyle {
        /// Style.
        style: StyleId,
    },

    // ------------------------------------------------------ definitions
    /// Creates an original passive industrial symbol using the shared typed model library.
    #[serde(rename_all = "camelCase")]
    CreateIndustrialSymbol {
        kind: hmi_project_model::model::v2_components::IndustrialSymbolKind,
    },
    /// Creates a reusable component definition.
    #[serde(rename_all = "camelCase")]
    CreateComponentDefinition {
        /// Definition (its ids must be new).
        definition: ComponentDefinition,
    },
    /// Updates name, description and category.
    #[serde(rename_all = "camelCase")]
    UpdateComponentDefinitionInfo {
        /// Definition.
        definition: ComponentDefinitionId,
        /// Name.
        name: String,
        /// Description.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// Palette category.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        category: Option<String>,
    },
    /// Replaces the parameter list.
    #[serde(rename_all = "camelCase")]
    SetComponentParameters {
        /// Definition.
        definition: ComponentDefinitionId,
        /// Parameters.
        parameters: Vec<ParameterDefinition>,
    },
    /// Deletes a definition. Fails validation while instances exist.
    #[serde(rename_all = "camelCase")]
    DeleteComponentDefinition {
        /// Definition.
        definition: ComponentDefinitionId,
    },
    /// Duplicates a definition with new ids (self-references remapped).
    #[serde(rename_all = "camelCase")]
    DuplicateComponentDefinition {
        /// Source definition.
        definition: ComponentDefinitionId,
        /// Name of the copy (default: `<name> copy`).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },

    // --------------------------------------------------------- project
    /// Atomically replace the typed industrial/security declarations.
    #[serde(rename_all = "camelCase")]
    SetIndustrialSecurity {
        declarations: hmi_project_model::model::v2::IndustrialSecurityDeclarations,
    },
    /// Atomically replace typed expressions and enumeration declarations.
    #[serde(rename_all = "camelCase")]
    SetExpressionEnumerations {
        declarations: hmi_project_model::model::v2_bindings::ExpressionEnumerationDeclarations,
    },
    /// Atomically replace component classifications and candidate action handlers.
    #[serde(rename_all = "camelCase")]
    SetComponentActions {
        declarations: hmi_project_model::model::v2_components::ComponentActionDeclarations,
    },
    /// Replaces project metadata.
    #[serde(rename_all = "camelCase")]
    SetMetadata {
        /// Metadata.
        metadata: ProjectMetadata,
    },
    /// Replaces project settings.
    #[serde(rename_all = "camelCase")]
    SetSettings {
        /// Settings.
        settings: ProjectSettings,
    },
    /// Replaces the navigation model.
    #[serde(rename_all = "camelCase")]
    SetNavigation {
        /// Navigation.
        navigation: Navigation,
    },
    /// Sets (`value`) or removes one project extension entry.
    #[serde(rename_all = "camelCase")]
    SetExtension {
        /// Namespaced key.
        key: String,
        /// Value; absent removes the entry.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<serde_json::Value>,
    },
}

impl Command {
    /// Stable operation name (the JSON `op` value).
    pub fn op_name(&self) -> &'static str {
        match self {
            Command::CreateScreen { .. } => "createScreen",
            Command::RenameScreen { .. } => "renameScreen",
            Command::SetScreenTitle { .. } => "setScreenTitle",
            Command::DeleteScreen { .. } => "deleteScreen",
            Command::DuplicateScreen { .. } => "duplicateScreen",
            Command::InsertNode { .. } => "insertNode",
            Command::InsertV2Component { .. } => "insertV2Component",
            Command::SetV2ComponentContract { .. } => "setV2ComponentContract",
            Command::RemoveNode { .. } => "removeNode",
            Command::DuplicateNode { .. } => "duplicateNode",
            Command::MoveNode { .. } => "moveNode",
            Command::ReorderNode { .. } => "reorderNode",
            Command::RenameNode { .. } => "renameNode",
            Command::SetElement { .. } => "setElement",
            Command::SetProperty { .. } => "setProperty",
            Command::SetNodeStyle { .. } => "setNodeStyle",
            Command::SetLayout { .. } => "setLayout",
            Command::SetResponsive { .. } => "setResponsive",
            Command::SetBinding { .. } => "setBinding",
            Command::RemoveBinding { .. } => "removeBinding",
            Command::SetEventHandler { .. } => "setEventHandler",
            Command::RemoveEventHandler { .. } => "removeEventHandler",
            Command::CreateTag { .. } => "createTag",
            Command::UpdateTag { .. } => "updateTag",
            Command::DeleteTag { .. } => "deleteTag",
            Command::UpsertStyle { .. } => "upsertStyle",
            Command::DeleteStyle { .. } => "deleteStyle",
            Command::CreateIndustrialSymbol { .. } => "createIndustrialSymbol",
            Command::CreateComponentDefinition { .. } => "createComponentDefinition",
            Command::UpdateComponentDefinitionInfo { .. } => "updateComponentDefinitionInfo",
            Command::SetComponentParameters { .. } => "setComponentParameters",
            Command::DeleteComponentDefinition { .. } => "deleteComponentDefinition",
            Command::DuplicateComponentDefinition { .. } => "duplicateComponentDefinition",
            Command::SetIndustrialSecurity { .. } => "setIndustrialSecurity",
            Command::SetExpressionEnumerations { .. } => "setExpressionEnumerations",
            Command::SetComponentActions { .. } => "setComponentActions",
            Command::SetMetadata { .. } => "setMetadata",
            Command::SetSettings { .. } => "setSettings",
            Command::SetNavigation { .. } => "setNavigation",
            Command::SetExtension { .. } => "setExtension",
        }
    }
}

/// How a transaction's result is validated before commit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationPolicy {
    /// Reject the transaction if it introduces any validation error that the
    /// project did not already have. On a valid project this is identical to
    /// `strict`; on an invalid project it allows incremental repair.
    #[default]
    RejectNewErrors,
    /// Reject the transaction unless the resulting project has no errors.
    Strict,
}

/// An atomic group of commands: all apply or none do. One undo step.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Transaction {
    /// Commands, applied in order to a working copy.
    pub commands: Vec<Command>,
    /// Optimistic concurrency: the revision the client based this on.
    /// Rejected if the session has moved on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<u64>,
    /// Undo-menu label (default: derived from the commands).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Consecutive transactions with the same merge key (e.g. one slider
    /// drag) collapse into a single undo step until another transaction,
    /// an undo/redo or a save intervenes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_key: Option<String>,
    /// Validation policy.
    #[serde(default)]
    pub validation: ValidationPolicy,
}

impl Transaction {
    /// Single-command transaction.
    pub fn single(command: Command) -> Self {
        Self {
            commands: vec![command],
            ..Self::default()
        }
    }

    /// Multi-command transaction.
    pub fn new(commands: Vec<Command>) -> Self {
        Self {
            commands,
            ..Self::default()
        }
    }

    /// Sets the expected revision.
    pub fn at_revision(mut self, revision: u64) -> Self {
        self.expected_revision = Some(revision);
        self
    }

    /// Sets the undo label.
    pub fn labeled(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets the merge key.
    pub fn merging(mut self, key: impl Into<String>) -> Self {
        self.merge_key = Some(key.into());
        self
    }

    /// Default undo label.
    pub fn display_label(&self) -> String {
        if let Some(l) = &self.label {
            return l.clone();
        }
        match self.commands.as_slice() {
            [one] => one.op_name().to_owned(),
            many => format!("{} changes", many.len()),
        }
    }
}

/// Map of identifiers allocated by a transaction: "request key" -> new id.
/// Keys: `screen`, `node:<original id>`, `definition`, ... (see docs).
pub type AllocatedIds = BTreeMap<String, String>;
