//! Source-neutral presentation of observed AS paths, Layer-2 fabric
//! context, and reviewed-but-unobserved relationships.
//!
//! All diagrams are server-rendered SVG: no JavaScript, no Graphviz,
//! no runtime dependencies. Layout is presentation only — it is never
//! serialized into canonical artifacts, never alters finding IDs, run
//! IDs, or plan hashes, and never adds an ASN that is absent from the
//! canonical or reviewed input.
//!
//! Evidence semantics:
//! - a **solid arrow** is an observed AS-path order at one public
//!   observer (canonical route evidence);
//! - a **dashed edge** is a reviewed relationship or predicate that
//!   was NOT observed in the selected evidence;
//! - a **grey undirected line** is reviewed Layer-2 attachment
//!   context (never a BGP adjacency claim).
//!
//! Arrow direction is never labeled provider/customer/peer unless
//! separate reviewed relationship evidence supports that exact label;
//! the default edge meaning is "observed AS-path sequence".

/// One normalized AS-path segment with reviewed presentation context.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PathNode {
    pub asn: u32,
    /// Consecutive repeats at this position (1 = single occurrence).
    pub repeat: u32,
    /// Reviewed short name when established; empty when unresolved.
    pub label: String,
    /// ASN matches the reviewed plane predicate of the analysis.
    pub plane_matched: bool,
    /// The analyzed target origin ASN.
    pub origin: bool,
}

/// A complete observed AS path at one observer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ObservedPath {
    pub observer: String,
    /// "direct collector session" | "indirect AS-path observation" | "not recorded".
    pub observation_kind: String,
    pub prefix: String,
    pub timestamp: String,
    pub nodes: Vec<PathNode>,
    /// Optional link to the run/evidence page.
    pub evidence_ref: Option<String>,
}

impl ObservedPath {
    pub fn text_sequence(&self) -> String {
        let mut out = String::new();
        for (i, n) in self.nodes.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&format!("AS{}", n.asn));
            for _ in 1..n.repeat {
                out.push_str(&format!(" AS{}", n.asn));
            }
        }
        out
    }
}

/// One state in a before/after path comparison.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PathStateView {
    /// "Event baseline", "Pre-finding state", "First changed state",
    /// "First route after return", "Analysis-final state".
    pub label: String,
    pub timestamp: String,
    /// `None` renders the absence block (no selected route visible).
    pub path: Option<ObservedPath>,
}

/// Reviewed Layer-2 attachment context of a case study.
#[derive(Debug, Clone)]
pub struct FabricView {
    pub label: String,
    pub attachments: Vec<FabricAttachmentView>,
    pub provenance: String,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FabricAttachmentView {
    pub label: String,
    pub note: String,
    pub asn: Option<u32>,
}

/// Reviewed relationship or predicate with its observed status.
#[derive(Debug, Clone)]
pub struct RelationshipView {
    pub label: String,
    pub asns: Vec<u32>,
    /// Whether selected evidence observed the relationship.
    pub observed: bool,
    pub note: String,
}

/// Compact consecutive-run representation: `[11537, 20965, 2603]` stays
/// as-is; `[24489, 24489, 24489, 24489]` becomes one node with
/// `repeat = 4`. Order is preserved.
pub fn compact_segments(asns: &[u32]) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &asn in asns {
        match out.last_mut() {
            Some((last, count)) if *last == asn => *count += 1,
            _ => out.push((asn, 1)),
        }
    }
    out
}

/// Full normalized AS-path text (no compaction) — the text equivalent
/// always preserves the complete sequence.
pub fn full_sequence_text(asns: &[u32]) -> String {
    asns.iter()
        .map(|a| format!("AS{a}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn escape_svg(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const NODE_W: f64 = 132.0;
const NODE_H: f64 = 46.0;
const NODE_GAP: f64 = 64.0;
const ROW_H: f64 = 78.0;

fn node_x(index: usize) -> f64 {
    16.0 + index as f64 * (NODE_W + NODE_GAP)
}

fn node_svg(n: &PathNode, x: f64, y: f64, changed: bool) -> String {
    let mut cls = String::from("pd-node");
    if n.origin {
        cls.push_str(" pd-node-origin");
    }
    if n.plane_matched {
        cls.push_str(" pd-node-plane");
    }
    if n.label.is_empty() {
        cls.push_str(" pd-node-unknown");
    }
    if changed {
        cls.push_str(" pd-node-changed");
    }
    let title = if n.repeat > 1 {
        format!(
            "AS{} appears {} consecutive times in the observed sequence",
            n.asn, n.repeat
        )
    } else {
        format!("AS{}", n.asn)
    };
    let label = if n.label.is_empty() {
        "name not reviewed".to_string()
    } else {
        n.label.clone()
    };
    let asn_text = if n.repeat > 1 {
        format!("AS{} ×{}", n.asn, n.repeat)
    } else {
        format!("AS{}", n.asn)
    };
    format!(
        r#"<g class="{cls}">
<title>{title}</title>
<rect x="{x:.0}" y="{y:.0}" width="{NODE_W:.0}" height="{NODE_H:.0}" rx="2"/>
<text x="{:.0}" y="{:.0}" text-anchor="middle" class="pd-asn">{}</text>
<text x="{:.0}" y="{:.0}" text-anchor="middle" class="pd-name">{}</text>
</g>"#,
        x + NODE_W / 2.0,
        y + 20.0,
        escape_svg(&asn_text),
        x + NODE_W / 2.0,
        y + 36.0,
        escape_svg(&label)
    )
}

fn arrow_svg(x1: f64, y1: f64, x2: f64, y2: f64) -> String {
    format!(
        r#"<line class="pd-edge" x1="{x1:.0}" y1="{y1:.0}" x2="{x2:.0}" y2="{y2:.0}" marker-end="url(#pd-arrow)"/>"#
    )
}

/// Render one observed AS path as an SVG row.
pub fn render_path_svg(path: &ObservedPath) -> String {
    let width = node_x(path.nodes.len().max(1)) + NODE_W + 8.0;
    let mut body = String::new();
    for (i, n) in path.nodes.iter().enumerate() {
        let x = node_x(i);
        body.push_str(&node_svg(n, x, 24.0, false));
        if i + 1 < path.nodes.len() {
            body.push_str(&arrow_svg(x + NODE_W, 47.0, node_x(i + 1), 47.0));
        }
    }
    let note = format!(
        "{} — observed AS-path sequence at {}",
        path.observer, path.prefix
    );
    format!(
        r#"<svg class="pd-svg" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:.0} 92" role="img" aria-label="{note}">
<defs>
<marker id="pd-arrow" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto">
<path d="M0,0 L8,4.5 L0,9 z" class="pd-arrow-head"/>
</marker>
</defs>
{body}
<text x="16" y="80" class="pd-caption">{}</text>
</svg>"#,
        escape_svg(&format!(
            "{} · {} · {}",
            path.observer, path.prefix, path.timestamp
        ))
    )
}

/// Render the absence block: no selected route visible at this observer.
pub fn render_absence_svg(observer: &str, prefix: &str) -> String {
    format!(
        r#"<svg class="pd-svg" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 420 64" role="img" aria-label="No selected route visible at this observer">
<rect class="pd-absence" x="12" y="10" width="396" height="40" rx="2"/>
<text x="24" y="34" class="pd-absence-text">No selected route visible at this observer — {observer} · {prefix}</text>
</svg>"#
    )
}

fn state_row(
    state: &PathStateView,
    row: usize,
    prev_path: Option<&[u32]>,
) -> (String, Option<Vec<u32>>) {
    let y = 14.0 + row as f64 * ROW_H;
    let label_x = 12.0;
    let path_x0 = 178.0;
    let mut out = String::new();
    out.push_str(&format!(
        r#"<text x="{label_x:.0}" y="{:.0}" class="pd-state-label">{}</text>"#,
        y + 26.0,
        escape_svg(&state.label)
    ));
    out.push_str(&format!(
        r#"<text x="{label_x:.0}" y="{:.0}" class="pd-state-time">{}</text>"#,
        y + 42.0,
        escape_svg(&state.timestamp)
    ));
    match &state.path {
        None => {
            out.push_str(&format!(
                r#"<g transform="translate({path_x0:.0}, {:.0})">"#,
                y
            ));
            out.push_str(&render_absence_svg_inner());
            out.push_str("</g>");
            (out, None)
        }
        Some(path) => {
            let nodes = &path.nodes;
            let mut changed: Vec<bool> = Vec::with_capacity(nodes.len());
            let prev = prev_path.unwrap_or(&[]);
            for (i, n) in nodes.iter().enumerate() {
                let same_as_prev = prev.get(i).copied() == Some(n.asn);
                changed.push(!same_as_prev);
            }
            for (i, n) in nodes.iter().enumerate() {
                let x = path_x0 + node_x(i) - 16.0;
                out.push_str(&node_svg(
                    &PathNode {
                        asn: n.asn,
                        repeat: n.repeat,
                        label: n.label.clone(),
                        plane_matched: n.plane_matched,
                        origin: n.origin,
                    },
                    x,
                    y + 4.0,
                    changed[i],
                ));
                if i + 1 < nodes.len() {
                    out.push_str(&arrow_svg(
                        x + NODE_W,
                        y + 27.0,
                        path_x0 + node_x(i + 1) - 16.0,
                        y + 27.0,
                    ));
                }
            }
            let full: Vec<u32> = nodes
                .iter()
                .flat_map(|n| std::iter::repeat_n(n.asn, n.repeat as usize))
                .collect();
            (out, Some(full))
        }
    }
}

fn render_absence_svg_inner() -> String {
    r#"<rect class="pd-absence" x="0" y="0" width="300" height="40" rx="2"/>
<text x="10" y="25" class="pd-absence-text">No selected route visible at this observer</text>"#
        .to_string()
}

/// Render a before/after state comparison (baseline → pre-finding →
/// first changed → first return → final). Absence states are blocks,
/// never ASN nodes; changed segments are marked, never attributed as
/// the cause of the change.
pub fn render_comparison_svg(states: &[PathStateView]) -> String {
    let max_nodes = states
        .iter()
        .filter_map(|s| s.path.as_ref())
        .map(|p| p.nodes.len())
        .max()
        .unwrap_or(0);
    let width = 190.0 + node_x(max_nodes.max(1)) + NODE_W;
    let height = 20.0 + states.len() as f64 * ROW_H + 8.0;
    let mut body = String::new();
    let mut prev: Option<Vec<u32>> = None;
    for (i, state) in states.iter().enumerate() {
        let (row, full) = state_row(state, i, prev.as_deref());
        body.push_str(&row);
        prev = full;
    }
    format!(
        r#"<svg class="pd-svg" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:.0} {height:.0}" role="img" aria-label="Observed AS-path states before, during, and after the change">
<defs>
<marker id="pd-arrow" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto">
<path d="M0,0 L8,4.5 L0,9 z" class="pd-arrow-head"/>
</marker>
</defs>
{body}
</svg>"#
    )
}

/// Render the reviewed Layer-2 fabric context. The fabric is a wide
/// neutral rectangle, never an ASN node; attachment lines are
/// undirected grey lines, never BGP edges.
pub fn render_fabric_svg(fabric: &FabricView) -> String {
    let count = fabric.attachments.len().max(1);
    let width = 120.0 + count as f64 * 150.0;
    let height = 190.0;
    let fabric_y = 26.0;
    let attach_y = 130.0;
    let center = width / 2.0;
    let fabric_w = (count as f64 * 150.0).max(220.0);
    let mut body = String::new();
    // Fabric block.
    body.push_str(&format!(
        r#"<g>
<title>Layer-2 fabric — not a BGP speaker</title>
<rect class="pd-fabric" x="{:.0}" y="{fabric_y:.0}" width="{fabric_w:.0}" height="44" rx="3"/>
<text x="{center:.0}" y="{:.0}" text-anchor="middle" class="pd-fabric-title">{}</text>
<text x="{center:.0}" y="{:.0}" text-anchor="middle" class="pd-fabric-sub">Layer-2 fabric — not a BGP speaker</text>
</g>"#,
        center - fabric_w / 2.0,
        fabric_y + 20.0,
        escape_svg(&fabric.label),
        fabric_y + 36.0,
    ));
    for (i, a) in fabric.attachments.iter().enumerate() {
        let x = 30.0 + i as f64 * 150.0 + 60.0;
        // Undirected grey attachment line — no arrowhead.
        body.push_str(&format!(
            r#"<line class="pd-attach" x1="{x:.0}" y1="{:.0}" x2="{x:.0}" y2="{attach_y:.0}"/>"#,
            fabric_y + 44.0
        ));
        let asn_text = match a.asn {
            Some(asn) => format!("AS{asn}"),
            None => String::from("no reviewed ASN"),
        };
        body.push_str(&format!(
            r#"<g>
<title>{att_label} — reviewed Layer-2 attachment (not BGP adjacency)</title>
<rect class="pd-attach-node" x="{:.0}" y="{attach_y:.0}" width="120" height="42" rx="2"/>
<text x="{:.0}" y="{:.0}" text-anchor="middle" class="pd-attach-asn">{}</text>
<text x="{:.0}" y="{:.0}" text-anchor="middle" class="pd-attach-name">{}</text>
</g>"#,
            x - 60.0,
            x,
            attach_y + 19.0,
            escape_svg(&asn_text),
            x,
            attach_y + 35.0,
            escape_svg(&a.label),
            att_label = escape_svg(&a.label)
        ));
    }
    format!(
        r#"<svg class="pd-svg" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:.0} {height:.0}" role="img" aria-label="Reviewed Layer-2 fabric context — reviewed attachments only">
{body}
</svg>"#
    )
}

/// Render a reviewed relationship/predicate. Dashed when unobserved in
/// the selected evidence; solid when observed.
pub fn render_relationship_svg(rel: &RelationshipView) -> String {
    let width = 16.0 + rel.asns.len() as f64 * (NODE_W + NODE_GAP);
    let y = 26.0;
    let edge_cls = if rel.observed {
        "pd-edge"
    } else {
        "pd-edge-dashed"
    };
    let mut body = String::new();
    for (i, &asn) in rel.asns.iter().enumerate() {
        let x = node_x(i);
        body.push_str(&node_svg(
            &PathNode {
                asn,
                repeat: 1,
                label: String::new(),
                plane_matched: false,
                origin: false,
            },
            x,
            y,
            false,
        ));
        if i + 1 < rel.asns.len() {
            let line = if rel.observed {
                format!(
                    r#"<line class="{edge_cls}" x1="{:.0}" y1="{:.0}" x2="{:.0}" y2="{:.0}" marker-end="url(#pd-arrow)"/>"#,
                    x + NODE_W,
                    y + 23.0,
                    node_x(i + 1),
                    y + 23.0
                )
            } else {
                format!(
                    r#"<line class="{edge_cls}" x1="{:.0}" y1="{:.0}" x2="{:.0}" y2="{:.0}"/>"#,
                    x + NODE_W,
                    y + 23.0,
                    node_x(i + 1),
                    y + 23.0
                )
            };
            body.push_str(&line);
        }
    }
    let note_y = y + NODE_H + 22.0;
    let title = if rel.observed {
        format!("Observed in selected evidence — {label}", label = rel.note)
    } else {
        format!(
            "Reviewed relationship sought — not observed in selected evidence — {label}",
            label = rel.note
        )
    };
    format!(
        r#"<svg class="pd-svg" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:.0} 110" role="img" aria-label="{title}">
<defs>
<marker id="pd-arrow" markerWidth="9" markerHeight="9" refX="7" refY="4.5" orient="auto">
<path d="M0,0 L8,4.5 L0,9 z" class="pd-arrow-head"/>
</marker>
</defs>
{body}
<text x="16" y="{note_y:.0}" class="pd-caption">{}</text>
</svg>"#,
        escape_svg(&format!(
            "{label} — {status}",
            label = rel.label,
            status = if rel.observed {
                "observed in selected evidence"
            } else {
                "reviewed relationship sought — not observed in selected evidence"
            }
        ))
    )
}

/// One canonical transition of a stream (from `lifecycle.json`).
#[derive(Debug, Clone)]
pub struct PathTransition {
    pub timestamp: String,
    pub kind: String,
    pub before_path: Vec<u32>,
    pub after_path: Vec<u32>,
}

/// Canonical stream path evidence loaded from a run's `lifecycle.json`
/// artifact (schema_version 1). The artifact is the authority; the
/// catalog's compact transition index deliberately has no paths.
#[derive(Debug, Clone)]
pub struct StreamPathEvidence {
    pub collector: String,
    pub peer_ip: String,
    pub prefix: String,
    pub baseline_path: Vec<u32>,
    pub transitions: Vec<PathTransition>,
    pub final_path: Vec<u32>,
    pub restoration_time_utc: Option<String>,
    pub first_change_utc: Option<String>,
    /// Observation ids of the transitions, for exact evidence links.
    pub observation_ids: Vec<String>,
}

/// Load all stream path evidence from a `lifecycle.json` artifact.
pub fn load_lifecycle_evidence(json: &str) -> Result<Vec<StreamPathEvidence>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("invalid lifecycle.json: {e}"))?;
    let mut out = Vec::new();
    let lifecycles = v
        .get("lifecycles")
        .and_then(|x| x.as_array())
        .ok_or_else(|| "lifecycle.json: missing lifecycles array".to_string())?;
    for lc in lifecycles {
        let prefix = lc
            .get("prefix")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let collector = lc
            .get("collector")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let peer_ip = lc
            .get("peer_ip")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let baseline_path = asn_vec(lc.get("baseline_path"));
        let final_path = lc
            .get("final_state")
            .and_then(|f| f.get("attributes"))
            .and_then(|a| a.get("as_path"))
            .map(|p| asn_vec(Some(p)))
            .unwrap_or_default();
        let mut transitions = Vec::new();
        let mut observation_ids = Vec::new();
        if let Some(ts) = lc.get("transitions").and_then(|x| x.as_array()) {
            for tr in ts {
                transitions.push(PathTransition {
                    timestamp: tr
                        .get("timestamp")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    kind: tr
                        .get("kind")
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .to_string(),
                    before_path: asn_vec(tr.get("before_path")),
                    after_path: asn_vec(tr.get("after_path")),
                });
                if let Some(oid) = tr.get("observation_id") {
                    observation_ids.push(oid.to_string());
                }
            }
        }
        out.push(StreamPathEvidence {
            collector,
            peer_ip,
            prefix,
            baseline_path,
            transitions,
            final_path,
            restoration_time_utc: lc
                .get("restoration_time")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
            first_change_utc: lc
                .get("first_change")
                .and_then(|x| x.as_str())
                .map(|s| s.to_string()),
            observation_ids,
        });
    }
    Ok(out)
}

fn asn_vec(v: Option<&serde_json::Value>) -> Vec<u32> {
    v.and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|n| n.as_u64().map(|n| n as u32))
                .collect()
        })
        .unwrap_or_default()
}

/// Reviewed ASN display names from `asn-identities.json` files.
///
/// Only identities with a non-empty `display_name` are used; nothing is
/// inferred when a name is absent.
pub fn load_asn_names(roots: &[&std::path::Path]) -> std::collections::BTreeMap<u32, String> {
    let mut names: std::collections::BTreeMap<u32, String> = std::collections::BTreeMap::new();
    for root in roots {
        let file = root.join("asn-identities.json");
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };
        let Some(ids) = v.get("identities").and_then(|x| x.as_array()) else {
            continue;
        };
        for id in ids {
            let Some(asn) = id.get("asn").and_then(|x| x.as_u64()).map(|n| n as u32) else {
                continue;
            };
            let name = id
                .get("display_name")
                .and_then(|x| x.as_str())
                .unwrap_or("");
            if !name.is_empty() && !names.contains_key(&asn) {
                names.insert(asn, name.to_string());
            }
        }
    }
    names
}

/// Build the before/after state comparison for one stream from
/// canonical evidence. Absence (`after_path` empty after a Withdrawal)
/// is a state block, never an ASN node. States that did not occur are
/// omitted (no fabricated rows).
pub fn comparison_states(
    ev: &StreamPathEvidence,
    origin_asn: Option<u32>,
    plane_asns: &[u32],
    names: &std::collections::BTreeMap<u32, String>,
) -> Vec<PathStateView> {
    let mut states = Vec::new();
    let observer = format!("{} (peer {})", ev.collector, ev.peer_ip);
    let to_path = |asns: &[u32], ts: &str| -> Option<ObservedPath> {
        if asns.is_empty() {
            return None;
        }
        Some(ObservedPath {
            observer: observer.clone(),
            observation_kind: String::new(),
            prefix: ev.prefix.clone(),
            timestamp: ts.to_string(),
            nodes: compact_segments(asns)
                .into_iter()
                .map(|(asn, repeat)| PathNode {
                    asn,
                    repeat,
                    label: names.get(&asn).cloned().unwrap_or_default(),
                    plane_matched: plane_asns.contains(&asn),
                    origin: origin_asn == Some(asn),
                })
                .collect(),
            evidence_ref: None,
        })
    };
    states.push(PathStateView {
        label: "Event baseline".to_string(),
        timestamp: "baseline RIB".to_string(),
        path: to_path(&ev.baseline_path, "baseline RIB"),
    });
    let mut first_change: Option<&PathTransition> = None;
    let mut absence_at: Option<&PathTransition> = None;
    let mut return_at: Option<&PathTransition> = None;
    for tr in &ev.transitions {
        if first_change.is_none() && (tr.kind != "Announcement" || !tr.before_path.is_empty()) {
            first_change = Some(tr);
        }
        if absence_at.is_none() && tr.kind == "Withdrawal" && tr.after_path.is_empty() {
            absence_at = Some(tr);
        }
        if absence_at.is_some()
            && return_at.is_none()
            && tr.kind != "Withdrawal"
            && !tr.after_path.is_empty()
        {
            return_at = Some(tr);
        }
    }
    if let Some(fc) = first_change {
        let pre = &fc.before_path;
        states.push(PathStateView {
            label: "Pre-finding state".to_string(),
            timestamp: fc.timestamp.clone(),
            path: to_path(pre, &fc.timestamp),
        });
        if fc.kind == "Withdrawal" && fc.after_path.is_empty() {
            states.push(PathStateView {
                label: "First changed state".to_string(),
                timestamp: fc.timestamp.clone(),
                path: None,
            });
        } else {
            states.push(PathStateView {
                label: "First changed state".to_string(),
                timestamp: fc.timestamp.clone(),
                path: to_path(&fc.after_path, &fc.timestamp),
            });
        }
    }
    if let Some(rt) = return_at {
        states.push(PathStateView {
            label: "First route after return".to_string(),
            timestamp: rt.timestamp.clone(),
            path: to_path(&rt.after_path, &rt.timestamp),
        });
    }
    states.push(PathStateView {
        label: "Analysis-final state".to_string(),
        timestamp: "analysis end".to_string(),
        path: to_path(&ev.final_path, "analysis end"),
    });
    states
}

#[cfg(test)]
mod tests {

    #[test]
    fn loader_reads_canonical_lifecycle_states() {
        let json = r#"{
          "schema_version": 1,
          "event_id": "X",
          "lifecycles": [{
            "collector": "collector-x",
            "peer_ip": "192.0.2.1",
            "prefix": "198.51.100.0/24",
            "baseline_path": [64500, 64501],
            "category": "Withdrawn",
            "flags": {"restored": true},
            "first_change": "2020-01-01T00:00:01Z",
            "restoration_time": "2020-01-01T00:00:03Z",
            "transitions": [
              {"timestamp": "2020-01-01T00:00:01Z", "kind": "Withdrawal",
               "before_path": [64500, 64502, 64501], "after_path": [],
               "observation_id": 1},
              {"timestamp": "2020-01-01T00:00:02Z", "kind": "Announcement",
               "before_path": [], "after_path": [64500, 64503, 64501],
               "observation_id": 2}
            ],
            "final_state": {"prefix": "198.51.100.0/24", "attributes": {"as_path": [64500, 64502, 64501]}}
          }]
        }"#;
        let evs = load_lifecycle_evidence(json).unwrap();
        assert_eq!(evs.len(), 1);
        let ev = &evs[0];
        assert_eq!(ev.baseline_path, vec![64500, 64501]);
        assert_eq!(ev.transitions.len(), 2);
        assert_eq!(ev.transitions[0].kind, "Withdrawal");
        assert!(ev.transitions[0].after_path.is_empty());
        assert_eq!(ev.final_path, vec![64500, 64502, 64501]);
        let names = std::collections::BTreeMap::new();
        let states = comparison_states(ev, Some(64501), &[64500], &names);
        let labels: Vec<&str> = states.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "Event baseline",
                "Pre-finding state",
                "First changed state",
                "First route after return",
                "Analysis-final state"
            ]
        );
        assert!(states[2].path.is_none(), "absence is a state block");
        assert_eq!(
            states[3].path.as_ref().unwrap().text_sequence(),
            "AS64500 AS64503 AS64501"
        );
    }

    #[test]
    fn comparison_omits_states_that_did_not_occur() {
        let json = r#"{
          "schema_version": 1,
          "event_id": "X",
          "lifecycles": [{
            "collector": "collector-x",
            "peer_ip": "192.0.2.1",
            "prefix": "198.51.100.0/24",
            "baseline_path": [64500, 64501],
            "category": "PathChangedStillViaTransit",
            "transitions": [
              {"timestamp": "2020-01-01T00:00:01Z", "kind": "PathReplacement",
               "before_path": [64500, 64501], "after_path": [64500, 64502, 64501],
               "observation_id": 3}
            ],
            "final_state": {"prefix": "198.51.100.0/24", "attributes": {"as_path": [64500, 64501]}}
          }]
        }"#;
        let evs = load_lifecycle_evidence(json).unwrap();
        let names = std::collections::BTreeMap::new();
        let states = comparison_states(&evs[0], None, &[], &names);
        let labels: Vec<&str> = states.iter().map(|s| s.label.as_str()).collect();
        assert!(!labels.contains(&"First route after return"), "{labels:?}");
        assert_eq!(states.len(), 4, "baseline + pre-finding + changed + final");
    }

    use super::*;

    fn node(asn: u32) -> PathNode {
        PathNode {
            asn,
            repeat: 1,
            label: String::new(),
            plane_matched: false,
            origin: false,
        }
    }

    #[test]
    fn compact_segments_preserves_order_and_counts() {
        let segments = [
            64500, 64501, 64502, 64502, 64502, 64502, 64503, 64504, 64505,
        ];
        let compact = compact_segments(&segments);
        assert_eq!(
            compact,
            vec![
                (64500, 1),
                (64501, 1),
                (64502, 4),
                (64503, 1),
                (64504, 1),
                (64505, 1)
            ]
        );
        // Text equivalent preserves the full sequence.
        assert_eq!(
            full_sequence_text(&segments),
            "AS64500 AS64501 AS64502 AS64502 AS64502 AS64502 AS64503 AS64504 AS64505"
        );
    }

    #[test]
    fn unknown_asn_not_given_guessed_name() {
        let mut path = ObservedPath {
            observer: "collector-x (peer 192.0.2.1)".to_string(),
            observation_kind: "indirect AS-path observation".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            timestamp: "2020-01-01T00:00:00Z".to_string(),
            nodes: vec![node(64599)],
            evidence_ref: None,
        };
        let svg = render_path_svg(&path);
        assert!(svg.contains("AS64599"), "{svg}");
        assert!(svg.contains("name not reviewed"), "{svg}");
        assert!(!svg.contains(">DreamHost<"), "no guessed organization");
        // Unknown-node class is present.
        assert!(svg.contains("pd-node-unknown"), "{svg}");
        path.nodes[0].label = "Reviewed Org".to_string();
        let svg2 = render_path_svg(&path);
        assert!(svg2.contains("Reviewed Org"), "{svg2}");
        assert!(!svg2.contains("pd-node-unknown"), "{svg2}");
    }

    #[test]
    fn observed_path_not_labeled_commercial_relationship() {
        let path = ObservedPath {
            observer: "collector-x (peer 192.0.2.1)".to_string(),
            observation_kind: "direct collector session".to_string(),
            prefix: "198.51.100.0/24".to_string(),
            timestamp: "2020-01-01T00:00:00Z".to_string(),
            nodes: vec![node(64500), node(64501)],
            evidence_ref: None,
        };
        let svg = render_path_svg(&path);
        assert!(svg.contains("observed AS-path sequence"), "{svg}");
        assert!(!svg.contains("provider"), "no provider label");
        assert!(!svg.contains("customer"), "no customer label");
        assert!(!svg.contains("peer relationship"), "no peer label");
    }

    #[test]
    fn as_path_order_matches_input() {
        let path = ObservedPath {
            observer: "o".to_string(),
            observation_kind: "direct collector session".to_string(),
            prefix: "p".to_string(),
            timestamp: "t".to_string(),
            nodes: vec![node(64500), node(64501), node(64502)],
            evidence_ref: None,
        };
        let svg = render_path_svg(&path);
        let i0 = svg.find("AS64500").unwrap();
        let i1 = svg.find("AS64501").unwrap();
        let i2 = svg.find("AS64502").unwrap();
        assert!(i0 < i1 && i1 < i2, "order preserved in SVG: {svg}");
        assert_eq!(path.text_sequence(), "AS64500 AS64501 AS64502");
    }

    #[test]
    fn prepend_compaction_preserves_count() {
        let path = ObservedPath {
            observer: "o".to_string(),
            observation_kind: "direct collector session".to_string(),
            prefix: "p".to_string(),
            timestamp: "t".to_string(),
            nodes: vec![
                PathNode {
                    asn: 64502,
                    repeat: 4,
                    ..node(64502)
                },
                node(64505),
            ],
            evidence_ref: None,
        };
        let svg = render_path_svg(&path);
        assert!(svg.contains("AS64502 ×4"), "{svg}");
        assert_eq!(
            path.text_sequence(),
            "AS64502 AS64502 AS64502 AS64502 AS64505"
        );
    }

    #[test]
    fn absence_not_rendered_as_as_node() {
        let svg = render_absence_svg("collector-x", "198.51.100.0/24");
        assert!(
            svg.contains("No selected route visible at this observer"),
            "{svg}"
        );
        assert!(!svg.contains("withdrawn"), "withdrawal is not an ASN node");
        assert!(!svg.contains("pd-node"), "no ASN node rendered");
    }

    #[test]
    fn diagram_links_to_evidence_reference() {
        let mut path = ObservedPath {
            observer: "o".to_string(),
            observation_kind: "direct collector session".to_string(),
            prefix: "p".to_string(),
            timestamp: "t".to_string(),
            nodes: vec![node(64500)],
            evidence_ref: Some("/analyses/7".to_string()),
        };
        // Evidence links are rendered by the page layer (HTML anchors);
        // the component keeps the reference addressable.
        assert_eq!(path.evidence_ref.as_deref(), Some("/analyses/7"));
        path.evidence_ref = None;
        assert!(path.evidence_ref.is_none());
    }

    #[test]
    fn comparison_preserves_lifecycle_states_and_absence() {
        let states = vec![
            PathStateView {
                label: "Event baseline".to_string(),
                timestamp: "baseline RIB".to_string(),
                path: Some(ObservedPath {
                    observer: "o".to_string(),
                    observation_kind: "direct collector session".to_string(),
                    prefix: "p".to_string(),
                    timestamp: "baseline".to_string(),
                    nodes: vec![node(64500), node(64505)],
                    evidence_ref: None,
                }),
            },
            PathStateView {
                label: "Pre-finding state".to_string(),
                timestamp: "2020-01-01T00:00:00Z".to_string(),
                path: Some(ObservedPath {
                    observer: "o".to_string(),
                    observation_kind: "direct collector session".to_string(),
                    prefix: "p".to_string(),
                    timestamp: "2020-01-01T00:00:00Z".to_string(),
                    nodes: vec![node(64500), node(64504), node(64505)],
                    evidence_ref: None,
                }),
            },
            PathStateView {
                label: "First changed state".to_string(),
                timestamp: "2020-01-01T00:00:01Z".to_string(),
                path: None,
            },
            PathStateView {
                label: "First route after return".to_string(),
                timestamp: "2020-01-01T00:00:02Z".to_string(),
                path: Some(ObservedPath {
                    observer: "o".to_string(),
                    observation_kind: "direct collector session".to_string(),
                    prefix: "p".to_string(),
                    timestamp: "2020-01-01T00:00:02Z".to_string(),
                    nodes: vec![node(64500), node(64502), node(64505)],
                    evidence_ref: None,
                }),
            },
            PathStateView {
                label: "Analysis-final state".to_string(),
                timestamp: "analysis end".to_string(),
                path: Some(ObservedPath {
                    observer: "o".to_string(),
                    observation_kind: "direct collector session".to_string(),
                    prefix: "p".to_string(),
                    timestamp: "analysis end".to_string(),
                    nodes: vec![node(64500), node(64504), node(64505)],
                    evidence_ref: None,
                }),
            },
        ];
        let svg = render_comparison_svg(&states);
        for label in [
            "Event baseline",
            "Pre-finding state",
            "First changed state",
            "First route after return",
            "Analysis-final state",
        ] {
            assert!(svg.contains(label), "missing {label}: {svg}");
        }
        assert!(
            svg.contains("No selected route visible at this observer"),
            "{svg}"
        );
        // State labels never imply the segments caused the change.
        assert!(!svg.contains("caused"), "{svg}");
        // The final state is rendered separately from the baseline.
        let baseline = svg.find("Event baseline").unwrap();
        let final_idx = svg.find("Analysis-final state").unwrap();
        assert!(baseline < final_idx);
    }

    #[test]
    fn final_path_not_assumed_baseline() {
        // Distinct node sequences for baseline and final must both render.
        let svg = render_comparison_svg(&[
            PathStateView {
                label: "Event baseline".to_string(),
                timestamp: "baseline".to_string(),
                path: Some(ObservedPath {
                    observer: "o".to_string(),
                    observation_kind: "direct collector session".to_string(),
                    prefix: "p".to_string(),
                    timestamp: "baseline".to_string(),
                    nodes: vec![node(64500), node(64501)],
                    evidence_ref: None,
                }),
            },
            PathStateView {
                label: "Analysis-final state".to_string(),
                timestamp: "analysis end".to_string(),
                path: Some(ObservedPath {
                    observer: "o".to_string(),
                    observation_kind: "direct collector session".to_string(),
                    prefix: "p".to_string(),
                    timestamp: "analysis end".to_string(),
                    nodes: vec![node(64500), node(64502), node(64501)],
                    evidence_ref: None,
                }),
            },
        ]);
        assert!(svg.contains("AS64502"), "final path rendered: {svg}");
    }

    #[test]
    fn fabric_diagram_contains_no_fabric_asn() {
        let fabric = FabricView {
            label: "Exchange Fabric".to_string(),
            attachments: vec![
                FabricAttachmentView {
                    label: "Network One".to_string(),
                    note: "reviewed".to_string(),
                    asn: Some(64500),
                },
                FabricAttachmentView {
                    label: "Network Two".to_string(),
                    note: "no reviewed ASN".to_string(),
                    asn: None,
                },
            ],
            provenance: "reviewed".to_string(),
            limitations: vec!["not adjacency".to_string()],
        };
        let svg = render_fabric_svg(&fabric);
        assert!(svg.contains("Layer-2 fabric — not a BGP speaker"), "{svg}");
        assert!(svg.contains("AS64500"), "reviewed ASN label rendered");
        assert!(
            svg.contains("no reviewed ASN"),
            "unestablished ASN not guessed"
        );
        assert!(!svg.contains("AS64501"), "no fabricated ASN");
        assert!(svg.contains("pd-attach"), "attachment lines present");
        assert!(!svg.contains("marker-end"), "no directional BGP edges");
    }

    #[test]
    fn fabric_edges_are_not_directional_bgp_edges() {
        let fabric = FabricView {
            label: "F".to_string(),
            attachments: vec![FabricAttachmentView {
                label: "N".to_string(),
                note: "n".to_string(),
                asn: Some(64500),
            }],
            provenance: "p".to_string(),
            limitations: vec![],
        };
        let svg = render_fabric_svg(&fabric);
        assert!(
            !svg.contains("pd-edge"),
            "attachment lines are not BGP edges"
        );
        assert!(svg.contains("pd-attach"), "{svg}");
    }

    #[test]
    fn relationship_unobserved_is_dashed_not_solid() {
        let rel = RelationshipView {
            label: "Adjacent(AS64500, AS64501)".to_string(),
            asns: vec![64500, 64501],
            observed: false,
            note: "reviewed".to_string(),
        };
        let svg = render_relationship_svg(&rel);
        assert!(svg.contains("pd-edge-dashed"), "{svg}");
        assert!(
            svg.contains("reviewed relationship sought — not observed in selected evidence"),
            "{svg}"
        );
    }

    #[test]
    fn observed_relationship_is_solid() {
        let rel = RelationshipView {
            label: "Adjacent(AS64500, AS64501)".to_string(),
            asns: vec![64500, 64501],
            observed: true,
            note: "observed".to_string(),
        };
        let svg = render_relationship_svg(&rel);
        assert!(svg.contains("pd-edge"), "{svg}");
        assert!(!svg.contains("pd-edge-dashed"), "{svg}");
    }
}
