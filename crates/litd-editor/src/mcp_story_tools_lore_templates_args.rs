use super::*;

pub(super) struct EntryArgs<'a> {
    pub(super) template: &'a LoreTemplateRequest,
    pub(super) ward: &'a LoreTemplateWardPolicy,
    pub(super) suffix: String,
    pub(super) kind: LoreEntryKind,
    pub(super) title: String,
    pub(super) summary: String,
    pub(super) body_rich_text: String,
    pub(super) ordinal: usize,
    pub(super) timeline_markers: Vec<LoreTimelineMarker>,
}
