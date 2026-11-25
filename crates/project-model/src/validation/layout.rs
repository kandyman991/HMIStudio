//! Layout-tree checks: hierarchy, layout constraints, responsive rules,
//! properties, styles, bindings, events and actions.

use std::collections::BTreeSet;

use super::checks::{at, finite_non_negative, Validator};
use super::values::{data_type_feeds, property_type_feeds, type_name};
use super::DiagnosticCode as C;
use crate::catalog::{self, Bindability};
use crate::model::*;
use crate::MAX_NODE_DEPTH;

/// Resolved binding target.
struct Target {
    ty: PropertyType,
    bindable: Bindability,
}

/// Resolved tag reference.
struct ResolvedTag {
    /// `None` for tag parameters without a declared data type.
    data_type: Option<DataType>,
    readable: bool,
    writable: bool,
}

fn component_kind(node: &Node) -> Option<ComponentKind> {
    match node.element {
        Element::Component { kind } => Some(kind),
        _ => None,
    }
}

fn is_flow(e: Option<&Element>) -> bool {
    matches!(e, Some(Element::Row { .. } | Element::Column { .. }))
}

impl<'p> Validator<'p> {
    /// Validates a whole tree rooted at `root`.
    pub fn check_tree(&mut self, root: &'p Node, path: &str, def: Option<&'p ComponentDefinition>) {
        self.check_node(root, path, None, 1, def);
    }

    fn check_node(
        &mut self,
        node: &'p Node,
        path: &str,
        parent: Option<&'p Element>,
        depth: usize,
        def: Option<&'p ComponentDefinition>,
    ) {
        if depth > MAX_NODE_DEPTH {
            self.error(
                C::NestingTooDeep,
                path,
                format!("layout nesting exceeds {MAX_NODE_DEPTH} levels; descendants not checked"),
            );
            return;
        }
        self.check_element(node, path);
        self.check_layout(node, path, parent);
        self.check_responsive(node, path, parent);
        self.check_properties(node, path);
        self.check_style(node, path);
        self.check_bindings(node, path, def);
        self.check_events(node, path, def);
        for (i, child) in node.children.iter().enumerate() {
            let cp = format!("{path}/children/{i}");
            self.check_node(child, &cp, Some(&node.element), depth + 1, def);
        }
    }

    // ------------------------------------------------------------- elements

    fn check_element(&mut self, node: &'p Node, path: &str) {
        let ep = at(path, "element");
        let n = node.children.len();
        match &node.element {
            Element::Row { spacing, .. } | Element::Column { spacing, .. } => {
                if !finite_non_negative(*spacing) {
                    self.error(
                        C::InvalidLayoutConstraint,
                        at(&ep, "spacing"),
                        "spacing must be >= 0",
                    );
                }
            }
            Element::Grid {
                columns,
                rows,
                column_gap,
                row_gap,
            } => {
                self.check_tracks(columns, &at(&ep, "columns"));
                self.check_tracks(rows, &at(&ep, "rows"));
                if !finite_non_negative(*column_gap) {
                    self.error(
                        C::InvalidLayoutConstraint,
                        at(&ep, "columnGap"),
                        "gap must be >= 0",
                    );
                }
                if !finite_non_negative(*row_gap) {
                    self.error(
                        C::InvalidLayoutConstraint,
                        at(&ep, "rowGap"),
                        "gap must be >= 0",
                    );
                }
            }
            Element::Scroll { .. } => {
                if n != 1 {
                    self.error(
                        C::InvalidHierarchy,
                        at(path, "children"),
                        format!("scroll needs exactly one child, found {n}"),
                    );
                }
            }
            Element::Box { .. } | Element::Absolute {} => {}
            Element::Component { kind } => {
                let max = kind.max_children();
                if n > max {
                    self.error(
                        C::InvalidHierarchy,
                        at(path, "children"),
                        format!(
                            "{} accepts at most {max} child(ren), found {n}",
                            kind.as_str()
                        ),
                    );
                }
            }
            Element::Instance {
                definition,
                parameters,
            } => {
                if n > 0 {
                    self.error(
                        C::InvalidHierarchy,
                        at(path, "children"),
                        "component instances cannot have children",
                    );
                }
                let Some(d) = self.ix.definitions.get(definition.as_str()).copied() else {
                    self.error(
                        C::UnknownReference,
                        at(&ep, "definition"),
                        format!("unknown component definition {definition:?}"),
                    );
                    return;
                };
                let pp = at(&ep, "parameters");
                for (name, value) in parameters {
                    match d.parameters.iter().find(|p| &p.name == name) {
                        None => self.error(
                            C::InvalidParameter,
                            at(&pp, name),
                            format!("{definition:?} has no parameter {name:?}"),
                        ),
                        Some(param) => self.check_value(value, &param.ty, &at(&pp, name)),
                    }
                }
                for param in &d.parameters {
                    let supplied = parameters.contains_key(&param.name)
                        || node.bindings.iter().any(|b| b.property == param.name);
                    if param.default.is_none() && !supplied {
                        self.error(
                            C::InvalidParameter,
                            &pp,
                            format!("required parameter {:?} is not supplied", param.name),
                        );
                    }
                }
            }
        }
    }

    fn check_tracks(&mut self, tracks: &[Track], path: &str) {
        if tracks.is_empty() {
            self.error(
                C::InvalidLayoutConstraint,
                path,
                "grid needs at least one track",
            );
        }
        for (i, t) in tracks.iter().enumerate() {
            let ok = match t {
                Track::Fixed { value } => value.is_finite() && *value > 0.0,
                Track::Fraction { weight } => weight.is_finite() && *weight > 0.0,
                Track::Auto {} => true,
            };
            if !ok {
                self.error(
                    C::InvalidLayoutConstraint,
                    at(path, i),
                    "track size/weight must be > 0",
                );
            }
        }
    }

    // --------------------------------------------------------------- layout

    fn check_size(&mut self, size: &Size, path: &str) {
        match size {
            Size::Auto {} => {}
            Size::Fill { fraction } => {
                if !(fraction.is_finite() && *fraction > 0.0 && *fraction <= 1.0) {
                    self.error(
                        C::InvalidLayoutConstraint,
                        path,
                        "fill fraction must be in (0, 1]",
                    );
                }
            }
            Size::Fixed { value } => {
                if !finite_non_negative(*value) {
                    self.error(C::InvalidLayoutConstraint, path, "fixed size must be >= 0");
                }
            }
        }
    }

    fn check_min_max(
        &mut self,
        size: Option<&Size>,
        min: Option<f64>,
        max: Option<f64>,
        path: &str,
        axis: &str,
    ) {
        for (name, v) in [("min", min), ("max", max)] {
            if let Some(v) = v {
                if !finite_non_negative(v) {
                    self.error(
                        C::InvalidLayoutConstraint,
                        at(path, format!("{name}{axis}")),
                        "constraint must be >= 0",
                    );
                }
            }
        }
        if let (Some(a), Some(b)) = (min, max) {
            if a > b {
                self.error(
                    C::InvalidLayoutConstraint,
                    at(path, format!("min{axis}")),
                    format!("min{axis} ({a}) exceeds max{axis} ({b})"),
                );
            }
        }
        if let Some(Size::Fixed { value }) = size {
            if min.is_some_and(|m| *value < m) || max.is_some_and(|m| *value > m) {
                self.error(
                    C::InvalidLayoutConstraint,
                    at(path, axis.to_ascii_lowercase()),
                    format!(
                        "fixed {} {value} violates its min/max constraints",
                        axis.to_ascii_lowercase()
                    ),
                );
            }
        }
    }

    fn check_insets(&mut self, insets: &Insets, path: &str) {
        for v in [insets.top, insets.end, insets.bottom, insets.start] {
            if !finite_non_negative(v) {
                self.error(C::InvalidLayoutConstraint, path, "padding must be >= 0");
                return;
            }
        }
    }

    fn check_grid_cell(&mut self, cell: &GridCell, parent: Option<&Element>, path: &str) {
        let Some(Element::Grid { columns, rows, .. }) = parent else {
            self.error(
                C::InvalidLayoutConstraint,
                path,
                "gridCell is only valid for children of a grid",
            );
            return;
        };
        if cell.column_span == 0 || cell.row_span == 0 {
            self.error(C::InvalidLayoutConstraint, path, "spans must be >= 1");
            return;
        }
        let col_end = u64::from(cell.column) + u64::from(cell.column_span);
        let row_end = u64::from(cell.row) + u64::from(cell.row_span);
        if col_end > columns.len() as u64 || row_end > rows.len() as u64 {
            self.error(
                C::InvalidLayoutConstraint,
                path,
                format!(
                    "cell (column {}, row {}, span {}x{}) exceeds the {}x{} grid",
                    cell.column,
                    cell.row,
                    cell.column_span,
                    cell.row_span,
                    columns.len(),
                    rows.len()
                ),
            );
        }
    }

    fn check_layout(&mut self, node: &Node, path: &str, parent: Option<&Element>) {
        let lp = &node.layout;
        let p = at(path, "layout");
        self.check_size(&lp.width, &at(&p, "width"));
        self.check_size(&lp.height, &at(&p, "height"));
        self.check_min_max(Some(&lp.width), lp.min_width, lp.max_width, &p, "Width");
        self.check_min_max(Some(&lp.height), lp.min_height, lp.max_height, &p, "Height");
        if let Some(r) = lp.aspect_ratio {
            if !(r.is_finite() && r > 0.0) {
                self.error(
                    C::InvalidLayoutConstraint,
                    at(&p, "aspectRatio"),
                    "aspectRatio must be > 0",
                );
            }
        }
        self.check_insets(&lp.padding, &at(&p, "padding"));

        if let Some(w) = lp.weight {
            if !is_flow(parent) {
                self.error(
                    C::InvalidLayoutConstraint,
                    at(&p, "weight"),
                    "weight is only valid for children of a row or column",
                );
            } else if !(w.is_finite() && w > 0.0) {
                self.error(
                    C::InvalidLayoutConstraint,
                    at(&p, "weight"),
                    "weight must be > 0",
                );
            }
        }
        if lp.cross_align.is_some() && !is_flow(parent) {
            self.error(
                C::InvalidLayoutConstraint,
                at(&p, "crossAlign"),
                "crossAlign is only valid for children of a row or column",
            );
        }
        if lp.align.is_some() && !matches!(parent, Some(Element::Box { .. })) {
            self.error(
                C::InvalidLayoutConstraint,
                at(&p, "align"),
                "align is only valid for children of a box",
            );
        }
        let in_absolute = matches!(parent, Some(Element::Absolute {}));
        match (&lp.position, in_absolute) {
            (Some(pos), true) => {
                if !(pos.x.is_finite() && pos.y.is_finite()) {
                    self.error(
                        C::InvalidLayoutConstraint,
                        at(&p, "position"),
                        "position must be finite",
                    );
                }
            }
            (Some(_), false) => self.error(
                C::InvalidLayoutConstraint,
                at(&p, "position"),
                "position is only valid for children of an absolute container",
            ),
            (None, true) => self.error(
                C::InvalidLayoutConstraint,
                &p,
                "children of an absolute container need a position",
            ),
            (None, false) => {}
        }
        match (&lp.grid_cell, parent) {
            (Some(cell), _) => self.check_grid_cell(cell, parent, &at(&p, "gridCell")),
            (None, Some(Element::Grid { .. })) => self.error(
                C::InvalidLayoutConstraint,
                &p,
                "children of a grid need a gridCell",
            ),
            (None, _) => {}
        }
        if let Some(Element::Scroll { direction }) = parent {
            let (v, h) = match direction {
                ScrollDirection::Vertical => (true, false),
                ScrollDirection::Horizontal => (false, true),
                ScrollDirection::Both => (true, true),
            };
            if v && matches!(lp.height, Size::Fill { .. }) {
                self.error(
                    C::InvalidLayoutConstraint,
                    at(&p, "height"),
                    "fill height inside a vertical scroll is unbounded; use auto or fixed",
                );
            }
            if h && matches!(lp.width, Size::Fill { .. }) {
                self.error(
                    C::InvalidLayoutConstraint,
                    at(&p, "width"),
                    "fill width inside a horizontal scroll is unbounded; use auto or fixed",
                );
            }
        }
    }

    // ----------------------------------------------------------- responsive

    fn check_responsive(&mut self, node: &Node, path: &str, parent: Option<&Element>) {
        for (i, rule) in node.responsive.iter().enumerate() {
            let rp = format!("{path}/responsive/{i}");
            let w = &rule.when;
            let wp = at(&rp, "when");
            if w.is_empty() {
                self.error(C::InvalidResponsiveRule, &wp, "rule has no condition");
            }
            if let Some(b) = &w.breakpoint {
                if !self.ix.breakpoints.contains(b.as_str()) {
                    self.error(
                        C::UnknownReference,
                        at(&wp, "breakpoint"),
                        format!("unknown breakpoint {b:?}"),
                    );
                }
            }
            if let Some(d) = &w.device_profile {
                if !self.ix.device_profiles.contains(d.as_str()) {
                    self.error(
                        C::UnknownReference,
                        at(&wp, "deviceProfile"),
                        format!("unknown device profile {d:?}"),
                    );
                }
            }
            for (name, v) in [("minWidth", w.min_width), ("maxWidth", w.max_width)] {
                if v.is_some_and(|v| !finite_non_negative(v)) {
                    self.error(
                        C::InvalidResponsiveRule,
                        at(&wp, name),
                        "width must be >= 0",
                    );
                }
            }
            if let (Some(a), Some(b)) = (w.min_width, w.max_width) {
                if a > b {
                    self.error(C::InvalidResponsiveRule, &wp, "minWidth exceeds maxWidth");
                }
            }

            let s = &rule.set;
            let sp = at(&rp, "set");
            if s.is_empty() {
                self.error(C::InvalidResponsiveRule, &sp, "rule overrides nothing");
            }
            if let Some(size) = &s.width {
                self.check_size(size, &at(&sp, "width"));
            }
            if let Some(size) = &s.height {
                self.check_size(size, &at(&sp, "height"));
            }
            self.check_min_max(s.width.as_ref(), s.min_width, s.max_width, &sp, "Width");
            self.check_min_max(s.height.as_ref(), s.min_height, s.max_height, &sp, "Height");
            if let Some(pad) = &s.padding {
                self.check_insets(pad, &at(&sp, "padding"));
            }
            let flow = matches!(node.element, Element::Row { .. } | Element::Column { .. });
            if s.direction.is_some() && !flow {
                self.error(
                    C::InvalidResponsiveRule,
                    at(&sp, "direction"),
                    "direction can only be overridden on rows and columns",
                );
            }
            if let Some(sp_v) = s.spacing {
                if !flow {
                    self.error(
                        C::InvalidResponsiveRule,
                        at(&sp, "spacing"),
                        "spacing can only be overridden on rows and columns",
                    );
                } else if !finite_non_negative(sp_v) {
                    self.error(
                        C::InvalidLayoutConstraint,
                        at(&sp, "spacing"),
                        "spacing must be >= 0",
                    );
                }
            }
            if let Some(cols) = &s.columns {
                if matches!(node.element, Element::Grid { .. }) {
                    self.check_tracks(cols, &at(&sp, "columns"));
                } else {
                    self.error(
                        C::InvalidResponsiveRule,
                        at(&sp, "columns"),
                        "columns can only be overridden on grids",
                    );
                }
            }
            if let Some(wt) = s.weight {
                if !is_flow(parent) {
                    self.error(
                        C::InvalidResponsiveRule,
                        at(&sp, "weight"),
                        "weight is only valid for children of a row or column",
                    );
                } else if !(wt.is_finite() && wt > 0.0) {
                    self.error(
                        C::InvalidLayoutConstraint,
                        at(&sp, "weight"),
                        "weight must be > 0",
                    );
                }
            }
            if let Some(cell) = &s.grid_cell {
                self.check_grid_cell(cell, parent, &at(&sp, "gridCell"));
            }
            if s.position.is_some() && !matches!(parent, Some(Element::Absolute {})) {
                self.error(
                    C::InvalidResponsiveRule,
                    at(&sp, "position"),
                    "position is only valid for children of an absolute container",
                );
            }
        }
    }

    // ----------------------------------------------------------- properties

    fn property_target(&self, node: &Node, name: &str) -> Option<Target> {
        match &node.element {
            Element::Instance { definition, .. } => {
                if let Some(spec) = catalog::find_property(None, name) {
                    return Some(Target {
                        ty: spec.ty,
                        bindable: spec.bindable,
                    });
                }
                let d = self.ix.definitions.get(definition.as_str())?;
                let param = d.parameters.iter().find(|p| p.name == name)?;
                if matches!(param.ty, PropertyType::Tag { .. }) {
                    return None;
                }
                Some(Target {
                    ty: param.ty.clone(),
                    bindable: Bindability::Read,
                })
            }
            _ => catalog::find_property(component_kind(node), name).map(|s| Target {
                ty: s.ty,
                bindable: s.bindable,
            }),
        }
    }

    fn check_properties(&mut self, node: &Node, path: &str) {
        let kind = component_kind(node);
        let pp = at(path, "properties");
        for (name, value) in &node.properties {
            match catalog::find_property(kind, name) {
                None => {
                    let what = match (&node.element, kind) {
                        (_, Some(k)) => k.as_str().to_owned(),
                        (Element::Instance { .. }, None) => {
                            "instances (set parameters in element.parameters)".to_owned()
                        }
                        (e, None) => e.type_name().to_owned(),
                    };
                    self.error(
                        C::UnknownProperty,
                        at(&pp, name),
                        format!("property {name:?} is not defined for {what}"),
                    );
                }
                Some(spec) => self.check_value(value, &spec.ty, &at(&pp, name)),
            }
        }
    }

    fn check_style(&mut self, node: &Node, path: &str) {
        let Some(style_id) = &node.style else {
            return;
        };
        match self.ix.styles.get(style_id.as_str()).copied() {
            None => self.error(
                C::UnknownReference,
                at(path, "style"),
                format!("unknown style {style_id:?}"),
            ),
            Some(style) => {
                if let Some(target) = style.target {
                    if component_kind(node) != Some(target) {
                        self.error(
                            C::InvalidStyle,
                            at(path, "style"),
                            format!(
                                "style {style_id:?} targets {} but the node is a {}",
                                target.as_str(),
                                component_kind(node)
                                    .map_or(node.element.type_name(), |k| k.as_str())
                            ),
                        );
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------- bindings

    fn resolve_tag_ref(
        &mut self,
        tag_ref: &TagRef,
        def: Option<&ComponentDefinition>,
        path: &str,
        code: C,
    ) -> Option<ResolvedTag> {
        match tag_ref {
            TagRef::Tag(id) => self.resolve_tag(id, path),
            TagRef::TagParameter(name) => self.resolve_tag_parameter(name, def, path, code),
        }
    }

    fn resolve_tag(&mut self, id: &TagId, path: &str) -> Option<ResolvedTag> {
        match self.ix.tags.get(id.as_str()).copied() {
            None => {
                self.error(C::UnknownReference, path, format!("unknown tag {id:?}"));
                None
            }
            Some(t) => Some(ResolvedTag {
                data_type: Some(t.data_type.clone()),
                readable: t.access.readable(),
                writable: t.access.writable(),
            }),
        }
    }

    fn resolve_tag_parameter(
        &mut self,
        name: &str,
        def: Option<&ComponentDefinition>,
        path: &str,
        code: C,
    ) -> Option<ResolvedTag> {
        let Some(d) = def else {
            self.error(
                code,
                path,
                format!("tagParameter {name:?} used outside a component definition"),
            );
            return None;
        };
        match d.parameters.iter().find(|p| p.name == name) {
            None => {
                self.error(code, path, format!("{:?} has no parameter {name:?}", d.id));
                None
            }
            Some(ParameterDefinition {
                ty:
                    PropertyType::Tag {
                        data_type,
                        writable,
                    },
                ..
            }) => Some(ResolvedTag {
                data_type: data_type.clone(),
                readable: true,
                writable: *writable,
            }),
            Some(p) => {
                self.error(
                    code,
                    path,
                    format!(
                        "parameter {name:?} is a {} parameter, not a tag",
                        type_name(&p.ty)
                    ),
                );
                None
            }
        }
    }

    fn check_bindings(&mut self, node: &Node, path: &str, def: Option<&ComponentDefinition>) {
        let mut seen = BTreeSet::new();
        for (j, b) in node.bindings.iter().enumerate() {
            let bp = format!("{path}/bindings/{j}");
            if !seen.insert(b.property.as_str()) {
                self.error(
                    C::DuplicateBinding,
                    at(&bp, "property"),
                    format!("property {:?} is bound more than once", b.property),
                );
                continue;
            }
            let Some(target) = self.property_target(node, &b.property) else {
                self.error(
                    C::UnknownProperty,
                    at(&bp, "property"),
                    format!("cannot bind unknown property {:?}", b.property),
                );
                continue;
            };
            if target.bindable == Bindability::Static {
                self.error(
                    C::InvalidBinding,
                    at(&bp, "property"),
                    format!("property {:?} cannot be bound", b.property),
                );
                continue;
            }
            if b.mode.writes() && target.bindable != Bindability::ReadWrite {
                self.error(
                    C::InvalidBinding,
                    at(&bp, "mode"),
                    format!(
                        "property {:?} is not an operator input; use mode read",
                        b.property
                    ),
                );
            }
            let sp = at(&bp, "source");
            let resolved = match &b.source {
                BindingSource::Tag(id) => self.resolve_tag(id, &sp),
                BindingSource::TagParameter(name) => {
                    self.resolve_tag_parameter(name, def, &sp, C::InvalidBinding)
                }
                BindingSource::Parameter(name) => {
                    self.check_parameter_binding(b, name, &target, def, &bp);
                    continue;
                }
            };
            let Some(resolved) = resolved else { continue };
            let data_type = match (&resolved.data_type, &b.member) {
                (Some(dt), member) => match self.resolve_member(dt, member.as_deref()) {
                    Ok(dt) => Some(dt),
                    Err(e) => {
                        self.error(C::InvalidDataType, at(&bp, "member"), e);
                        continue;
                    }
                },
                (None, _) => None,
            };
            if b.mode.reads() && !resolved.readable {
                self.error(
                    C::InvalidBinding,
                    at(&bp, "mode"),
                    "source tag is not readable",
                );
            }
            if b.mode.writes() && !resolved.writable {
                self.error(
                    C::InvalidBinding,
                    at(&bp, "mode"),
                    "binding writes to a tag the HMI may not write",
                );
            }
            self.check_transform(b, data_type.as_ref(), &target.ty, &bp);
        }
    }

    fn check_parameter_binding(
        &mut self,
        b: &PropertyBinding,
        name: &str,
        target: &Target,
        def: Option<&ComponentDefinition>,
        bp: &str,
    ) {
        let sp = at(bp, "source");
        let Some(d) = def else {
            self.error(
                C::InvalidBinding,
                sp,
                format!("parameter {name:?} used outside a component definition"),
            );
            return;
        };
        let Some(param) = d.parameters.iter().find(|p| p.name == name) else {
            self.error(
                C::InvalidBinding,
                sp,
                format!("{:?} has no parameter {name:?}", d.id),
            );
            return;
        };
        if matches!(param.ty, PropertyType::Tag { .. }) {
            self.error(
                C::InvalidBinding,
                sp,
                format!("{name:?} is a tag parameter; use the tagParameter source"),
            );
            return;
        }
        if b.mode != BindingMode::Read {
            self.error(
                C::InvalidBinding,
                at(bp, "mode"),
                "parameter bindings are read-only",
            );
        }
        if b.member.is_some() || b.transform.is_some() {
            self.error(
                C::InvalidBinding,
                bp,
                "parameter bindings support neither member paths nor transforms",
            );
        }
        if !property_type_feeds(&param.ty, &target.ty) {
            self.error(
                C::InvalidBinding,
                sp,
                format!(
                    "{} parameter {name:?} cannot feed a {} property",
                    type_name(&param.ty),
                    type_name(&target.ty)
                ),
            );
        }
    }

    fn check_transform(
        &mut self,
        b: &PropertyBinding,
        dt: Option<&DataType>,
        ty: &PropertyType,
        bp: &str,
    ) {
        let tp = at(bp, "transform");
        let numeric_prop = matches!(
            ty,
            PropertyType::Number { .. } | PropertyType::Integer { .. }
        );
        match &b.transform {
            None => {
                if let Some(dt) = dt {
                    if !data_type_feeds(dt, ty) {
                        self.error(
                            C::InvalidBinding,
                            at(bp, "source"),
                            format!(
                                "a {} value cannot drive a {} property without a transform",
                                dt.describe(),
                                type_name(ty)
                            ),
                        );
                    }
                }
            }
            Some(ValueTransform::Linear { factor, offset }) => {
                if !(factor.is_finite() && offset.is_finite()) {
                    self.error(C::InvalidBinding, &tp, "factor and offset must be finite");
                }
                if b.mode.writes() && *factor == 0.0 {
                    self.error(
                        C::InvalidBinding,
                        &tp,
                        "a zero factor cannot be inverted for writes",
                    );
                }
                if dt.is_some_and(|d| !d.is_numeric()) || !numeric_prop {
                    self.error(
                        C::InvalidBinding,
                        &tp,
                        "linear needs a numeric source and property",
                    );
                }
            }
            Some(ValueTransform::Invert {}) => {
                if dt.is_some_and(|d| *d != DataType::Bool) || *ty != (PropertyType::Bool {}) {
                    self.error(
                        C::InvalidBinding,
                        &tp,
                        "invert needs a bool source and property",
                    );
                }
            }
            Some(ValueTransform::Bit { index }) => {
                if b.mode.writes() {
                    self.error(C::InvalidBinding, &tp, "bit extraction is read-only");
                }
                if *ty != (PropertyType::Bool {}) {
                    self.error(C::InvalidBinding, &tp, "bit needs a bool property");
                }
                if let Some(d) = dt {
                    match d.integer_bits() {
                        Some(bits) if *index < bits => {}
                        Some(bits) => self.error(
                            C::InvalidBinding,
                            at(&tp, "index"),
                            format!("bit {index} out of range for a {bits}-bit value"),
                        ),
                        None => self.error(C::InvalidBinding, &tp, "bit needs an integer source"),
                    }
                }
            }
            Some(ValueTransform::Map { entries, default }) => {
                if b.mode.writes() {
                    self.error(C::InvalidBinding, &tp, "map transforms are read-only");
                }
                if entries.is_empty() {
                    self.error(C::InvalidBinding, at(&tp, "entries"), "map has no entries");
                }
                for (k, e) in entries.iter().enumerate() {
                    let ep = format!("{tp}/entries/{k}");
                    if let Some(d) = dt {
                        if let Err(msg) = self.value_matches_data_type(&e.when, d) {
                            self.error(C::InvalidBinding, at(&ep, "when"), msg);
                        }
                    }
                    self.check_value(&e.value, ty, &at(&ep, "value"));
                }
                if let Some(v) = default {
                    self.check_value(v, ty, &at(&tp, "default"));
                }
            }
        }
    }

    // --------------------------------------------------------------- events

    fn check_events(&mut self, node: &Node, path: &str, def: Option<&ComponentDefinition>) {
        let allowed = catalog::events(component_kind(node));
        let mut seen = BTreeSet::new();
        for (j, h) in node.events.iter().enumerate() {
            let hp = format!("{path}/events/{j}");
            if !allowed.contains(&h.event) {
                self.error(
                    C::InvalidEvent,
                    at(&hp, "event"),
                    format!(
                        "{} does not raise {:?} (allowed: {allowed:?})",
                        component_kind(node).map_or(node.element.type_name(), |k| k.as_str()),
                        h.event
                    ),
                );
            }
            if !seen.insert(h.event) {
                self.error(
                    C::InvalidEvent,
                    at(&hp, "event"),
                    format!(
                        "more than one handler for {:?}; merge their actions",
                        h.event
                    ),
                );
            }
            self.check_handler(h, &hp, def);
        }
    }

    /// Checks a handler's confirmation text and actions.
    pub fn check_handler(
        &mut self,
        h: &EventHandler,
        path: &str,
        def: Option<&ComponentDefinition>,
    ) {
        if let Some(c) = &h.confirm {
            self.check_text(c, &at(path, "confirm"));
        }
        if h.actions.is_empty() {
            self.error(
                C::InvalidEvent,
                at(path, "actions"),
                "handler has no actions",
            );
        }
        for (k, a) in h.actions.iter().enumerate() {
            self.check_action(a, &format!("{path}/actions/{k}"), def);
        }
    }

    fn writable_target(
        &mut self,
        target: &TagRef,
        member: Option<&str>,
        def: Option<&ComponentDefinition>,
        path: &str,
        requires_read: bool,
    ) -> Option<Option<DataType>> {
        let tp = at(path, "target");
        let r = self.resolve_tag_ref(target, def, &tp, C::InvalidAction)?;
        if !r.writable {
            self.error(
                C::InvalidAction,
                &tp,
                "action writes to a tag the HMI may not write",
            );
        }
        if requires_read && !r.readable {
            self.error(
                C::InvalidAction,
                &tp,
                "read-modify-write action needs a readable tag",
            );
        }
        match r.data_type {
            None => Some(None),
            Some(dt) => match self.resolve_member(&dt, member) {
                Ok(dt) => Some(Some(dt)),
                Err(e) => {
                    self.error(C::InvalidDataType, at(path, "member"), e);
                    None
                }
            },
        }
    }

    pub(super) fn check_action(
        &mut self,
        a: &Action,
        path: &str,
        def: Option<&ComponentDefinition>,
    ) {
        match a {
            Action::Navigate { screen } => {
                self.check_screen_ref(
                    screen,
                    ScreenKind::Page,
                    &at(path, "screen"),
                    C::InvalidAction,
                );
            }
            Action::OpenPopup { screen } => {
                self.check_screen_ref(
                    screen,
                    ScreenKind::Popup,
                    &at(path, "screen"),
                    C::InvalidAction,
                );
            }
            Action::Back {} | Action::ClosePopup {} => {}
            Action::WriteTag {
                target,
                member,
                value,
            } => {
                if let Some(Some(dt)) =
                    self.writable_target(target, member.as_deref(), def, path, false)
                {
                    if let Err(e) = self.value_matches_data_type(value, &dt) {
                        self.error(C::InvalidAction, at(path, "value"), e);
                    }
                }
            }
            Action::ToggleTag { target, member } => {
                if let Some(Some(dt)) =
                    self.writable_target(target, member.as_deref(), def, path, true)
                {
                    if dt != DataType::Bool {
                        self.error(
                            C::InvalidAction,
                            at(path, "target"),
                            "toggleTag needs a bool tag",
                        );
                    }
                }
            }
            Action::PulseTag {
                target,
                member,
                duration_ms,
            } => {
                if *duration_ms == 0 {
                    self.error(
                        C::InvalidAction,
                        at(path, "durationMs"),
                        "duration must be > 0",
                    );
                }
                if let Some(Some(dt)) =
                    self.writable_target(target, member.as_deref(), def, path, false)
                {
                    if dt != DataType::Bool {
                        self.error(
                            C::InvalidAction,
                            at(path, "target"),
                            "pulseTag needs a bool tag",
                        );
                    }
                }
            }
            Action::StepTag {
                target,
                member,
                step,
                min,
                max,
            } => {
                if !(step.is_finite() && *step != 0.0) {
                    self.error(
                        C::InvalidAction,
                        at(path, "step"),
                        "step must be finite and non-zero",
                    );
                }
                for (name, bound) in [("min", min), ("max", max)] {
                    if bound.is_some_and(|value| !value.is_finite()) {
                        self.error(C::InvalidAction, at(path, name), "clamp must be finite");
                    }
                }
                if let (Some(a), Some(b)) = (min, max) {
                    if a > b {
                        self.error(C::InvalidAction, at(path, "min"), "min exceeds max");
                    }
                }
                if let Some(Some(dt)) =
                    self.writable_target(target, member.as_deref(), def, path, true)
                {
                    if !dt.is_numeric() {
                        self.error(
                            C::InvalidAction,
                            at(path, "target"),
                            "stepTag needs a numeric tag",
                        );
                    }
                }
            }
            Action::AcknowledgeAlarms { alarm, group } => {
                if alarm.is_some() && group.is_some() {
                    self.error(
                        C::InvalidAction,
                        path,
                        "set either alarm or group, not both",
                    );
                }
                if let Some(id) = alarm {
                    if !self.ix.alarms.contains(id.as_str()) {
                        self.error(
                            C::UnknownReference,
                            at(path, "alarm"),
                            format!("unknown alarm {id:?}"),
                        );
                    }
                }
                if let Some(g) = group {
                    if !self.ix.alarm_groups.contains(g.as_str()) {
                        self.error(
                            C::UnknownReference,
                            at(path, "group"),
                            format!("no alarm uses group {g:?}"),
                        );
                    }
                }
            }
            Action::LoadRecipe { recipe, set } | Action::SaveRecipe { recipe, set } => {
                match self.ix.recipes.get(recipe.as_str()).copied() {
                    None => self.error(
                        C::UnknownReference,
                        at(path, "recipe"),
                        format!("unknown recipe {recipe:?}"),
                    ),
                    Some(r) => {
                        if matches!(a, Action::SaveRecipe { .. }) {
                            for field in &r.fields {
                                if self
                                    .ix
                                    .tags
                                    .get(field.tag.as_str())
                                    .is_some_and(|tag| !tag.access.readable())
                                {
                                    self.error(
                                        C::InvalidAction,
                                        at(path, "recipe"),
                                        format!(
                                            "recipe field {:?} uses a non-readable tag",
                                            field.id
                                        ),
                                    );
                                }
                            }
                        }
                        if let Some(s) = set {
                            if !r.sets.iter().any(|x| &x.id == s) {
                                self.error(
                                    C::UnknownReference,
                                    at(path, "set"),
                                    format!("recipe {recipe:?} has no set {s:?}"),
                                );
                            }
                        }
                    }
                }
            }
            Action::SetLocale { locale } => {
                if !self.ix.locales.contains(locale.as_str()) {
                    self.error(
                        C::UnknownReference,
                        at(path, "locale"),
                        format!("locale {locale:?} is not declared"),
                    );
                }
            }
        }
    }
}
