use std::marker::PhantomData;

use minimax_protocol::IndexDomain;
use serde::{Deserialize, Serialize};

pub trait DomainMarker: Clone + std::fmt::Debug + Send + Sync + 'static {
    const DOMAIN: IndexDomain;
}

#[derive(Clone, Debug)]
pub struct CapabilityMarker;

impl DomainMarker for CapabilityMarker {
    const DOMAIN: IndexDomain = IndexDomain::Capability;
}

#[derive(Clone, Debug)]
pub struct ProjectMarker;

impl DomainMarker for ProjectMarker {
    const DOMAIN: IndexDomain = IndexDomain::Project;
}

#[derive(Clone, Debug)]
pub struct SkillMarker;

impl DomainMarker for SkillMarker {
    const DOMAIN: IndexDomain = IndexDomain::Skill;
}

#[derive(Clone, Debug)]
pub struct McpMarker;

impl DomainMarker for McpMarker {
    const DOMAIN: IndexDomain = IndexDomain::Mcp;
}

#[derive(Clone, Debug)]
pub struct WikiMarker;

impl DomainMarker for WikiMarker {
    const DOMAIN: IndexDomain = IndexDomain::Wiki;
}

pub trait SearchDocument: Clone + std::fmt::Debug + Send + Sync {
    type Marker: DomainMarker;

    fn id(&self) -> &str;
    fn title(&self) -> &str;
    fn exact_keys(&self) -> Vec<&str>;
    fn search_text(&self) -> String;
    fn is_searchable(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityDocument {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    pub intent_document: String,
    #[serde(default = "default_true")]
    pub available: bool,
}

impl SearchDocument for CapabilityDocument {
    type Marker = CapabilityMarker;

    fn id(&self) -> &str {
        &self.id
    }

    fn title(&self) -> &str {
        &self.name
    }

    fn exact_keys(&self) -> Vec<&str> {
        std::iter::once(self.id.as_str())
            .chain(std::iter::once(self.name.as_str()))
            .chain(self.aliases.iter().map(String::as_str))
            .chain(self.commands.iter().map(String::as_str))
            .collect()
    }

    fn search_text(&self) -> String {
        self.intent_document.clone()
    }

    fn is_searchable(&self) -> bool {
        self.available
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectDocument {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub description: String,
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
}

impl SearchDocument for ProjectDocument {
    type Marker = ProjectMarker;

    fn id(&self) -> &str {
        &self.id
    }

    fn title(&self) -> &str {
        &self.name
    }

    fn exact_keys(&self) -> Vec<&str> {
        std::iter::once(self.id.as_str())
            .chain(std::iter::once(self.name.as_str()))
            .chain(self.aliases.iter().map(String::as_str))
            .collect()
    }

    fn search_text(&self) -> String {
        std::iter::once(self.name.as_str())
            .chain(std::iter::once(self.description.as_str()))
            .chain(self.aliases.iter().map(String::as_str))
            .chain(self.topics.iter().map(String::as_str))
            .chain(self.platforms.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

macro_rules! external_capability_document {
    ($name:ident, $marker:ty) => {
        #[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct $name {
            pub id: String,
            pub name: String,
            #[serde(default)]
            pub aliases: Vec<String>,
            pub description: String,
            #[serde(default)]
            pub intents: Vec<String>,
            #[serde(default)]
            pub platforms: Vec<String>,
        }

        impl SearchDocument for $name {
            type Marker = $marker;

            fn id(&self) -> &str {
                &self.id
            }

            fn title(&self) -> &str {
                &self.name
            }

            fn exact_keys(&self) -> Vec<&str> {
                std::iter::once(self.id.as_str())
                    .chain(std::iter::once(self.name.as_str()))
                    .chain(self.aliases.iter().map(String::as_str))
                    .collect()
            }

            fn search_text(&self) -> String {
                std::iter::once(self.name.as_str())
                    .chain(std::iter::once(self.description.as_str()))
                    .chain(self.aliases.iter().map(String::as_str))
                    .chain(self.intents.iter().map(String::as_str))
                    .chain(self.platforms.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
    };
}

external_capability_document!(SkillDocument, SkillMarker);
external_capability_document!(McpDocument, McpMarker);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WikiDocument {
    pub id: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub current: bool,
}

impl SearchDocument for WikiDocument {
    type Marker = WikiMarker;

    fn id(&self) -> &str {
        &self.id
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn exact_keys(&self) -> Vec<&str> {
        std::iter::once(self.id.as_str())
            .chain(std::iter::once(self.title.as_str()))
            .chain(self.aliases.iter().map(String::as_str))
            .collect()
    }

    fn search_text(&self) -> String {
        format!("{}\n{}", self.title, self.body)
    }

    fn is_searchable(&self) -> bool {
        self.current
    }
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug)]
pub(crate) struct DomainIdentity<M: DomainMarker>(pub(crate) PhantomData<M>);

impl<M: DomainMarker> Default for DomainIdentity<M> {
    fn default() -> Self {
        Self(PhantomData)
    }
}
