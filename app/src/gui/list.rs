// SPDX-License-Identifier: MIT
//! The catalog as a tree of type, family and item rows, like the terminal browser's.

use std::collections::{BTreeMap, HashSet};

use bof3player::catalog::{Filter, Item, MediaType};
use bof3player::library::Library;

/// Most search hits listed.
const MAX_HITS: usize = 5000;
/// The text of an item row: its title or name, else its id.
pub fn label(item: &Item) -> &str {
    [&item.title, &item.name, &item.id].into_iter().find(|text| !text.is_empty()).map_or("", |text| text.as_str())
}

/// One line of the list.
#[derive(Clone, Copy)]
pub enum Row {
    /// A type and the items under it.
    MediaType(MediaType, usize),
    /// A family of one type and the items under it.
    Family(MediaType, &'static str, usize),
    /// An item.
    Item(&'static Item),
}

impl Row {
    /// A key that names the row across rebuilds.
    pub fn key(&self) -> String {
        match self {
            Row::MediaType(media_type, _) => media_type_key(*media_type),
            Row::Family(media_type, family, _) => family_key(*media_type, family),
            Row::Item(item) => item.id.clone(),
        }
    }
}

fn media_type_key(media_type: MediaType) -> String {
    media_type.name().to_string()
}

fn family_key(media_type: MediaType, family: &str) -> String {
    format!("{}/{family}", media_type.name())
}

/// All items grouped by type and family, and which groups are open.
pub struct Tree {
    library: &'static Library<'static>,
    groups: BTreeMap<MediaType, BTreeMap<&'static str, Vec<&'static Item>>>,
    expanded: HashSet<String>,
    /// How many playable items the catalog holds.
    pub playable: usize,
}

impl Tree {
    /// Group the catalog of `library`. Music and its families start open.
    pub fn new(library: &'static Library<'static>) -> Self {
        let mut groups: BTreeMap<MediaType, BTreeMap<&str, Vec<&Item>>> = BTreeMap::new();
        for item in library.catalog().items() {
            groups.entry(item.media_type).or_default().entry(item.family.as_str()).or_default().push(item);
        }
        let mut expanded = HashSet::new();
        if let Some(families) = groups.get(&MediaType::Music) {
            expanded.insert(media_type_key(MediaType::Music));
            expanded.extend(families.keys().map(|family| family_key(MediaType::Music, family)));
        }
        Self { library, groups, expanded, playable: library.catalog().playable_count() }
    }

    /// The number of catalog items.
    pub fn total(&self) -> usize {
        self.library.catalog().len()
    }

    /// Whether the group named by `row` is open.
    pub fn is_open(&self, row: &Row) -> bool {
        self.expanded.contains(&row.key())
    }

    /// Open or close the group `row` names; `open` forces the state. Items do not toggle.
    pub fn set_open(&mut self, row: &Row, open: Option<bool>) {
        if matches!(row, Row::Item(_)) {
            return;
        }
        let key = row.key();
        let open = open.unwrap_or(!self.expanded.contains(&key));
        if open {
            self.expanded.insert(key);
        } else {
            self.expanded.remove(&key);
        }
    }

    /// The visible rows. A non-empty `query` lists the ranked hits under their types instead of
    /// the families.
    pub fn rows(&self, query: &str, playable_only: bool) -> Vec<Row> {
        let query = query.trim();
        let mut rows = Vec::new();
        if query.is_empty() {
            for media_type in MediaType::LIST {
                let Some(families) = self.groups.get(&media_type) else { continue };
                let count = |items: &Vec<&Item>| items.iter().filter(|item| !playable_only || item.playable).count();
                let total: usize = families.values().map(count).sum();
                if total == 0 {
                    continue;
                }
                rows.push(Row::MediaType(media_type, total));
                if !self.expanded.contains(&media_type_key(media_type)) {
                    continue;
                }
                for (&family, items) in families {
                    let shown = count(items);
                    if shown == 0 {
                        continue;
                    }
                    rows.push(Row::Family(media_type, family, shown));
                    if self.expanded.contains(&family_key(media_type, family)) {
                        rows.extend(items.iter().filter(|item| !playable_only || item.playable).map(|&i| Row::Item(i)));
                    }
                }
            }
            return rows;
        }
        let filter = Filter { playable_only, ..Filter::default() };
        let hits = self.library.catalog().search(query, &filter);
        for media_type in MediaType::LIST {
            let items: Vec<&Item> = hits.iter().copied().filter(|item| item.media_type == media_type).take(MAX_HITS).collect();
            if !items.is_empty() {
                rows.push(Row::MediaType(media_type, items.len()));
                rows.extend(items.into_iter().map(Row::Item));
            }
        }
        rows
    }
}
