//! Token carriage policy: how much literal source text a UAST carries.
//!
//! Structure alone is always safe to send anywhere. Tokens are where the risk lives — but
//! also much of the meaning, so a blanket ban costs real signal. A type name (`int`) and a
//! string literal (`"sk-live-…"`) are not remotely the same risk, and treating them
//! identically forces the user to choose between a useless UAST and an unsafe one.
//!
//! So the policy is per-CATEGORY, and the default is [`TokenPolicy::none`] — opt IN to
//! disclosure, never out of it. A misconfiguration should fail closed.
//!
//! This generalises the existing three-level share model:
//!
//! | Share level  | Equivalent policy                                      |
//! |--------------|--------------------------------------------------------|
//! | `facts`      | [`TokenPolicy::none`] — structure only                  |
//! | `signatures` | [`TokenPolicy::signatures`] — names and types, no values|
//! | `full`       | [`TokenPolicy::all`] — everything, local endpoints only |

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::Category;

/// Which categories may carry their source token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenPolicy {
    allowed: BTreeSet<Category>,
    /// Escape hatch for local-only use; keeps `allowed` meaningful rather than requiring
    /// every category to be listed.
    all: bool,
}

impl Default for TokenPolicy {
    /// Structure only. Privacy-safe by construction, so forgetting to configure a policy
    /// cannot leak anything.
    fn default() -> Self {
        Self::none()
    }
}

impl TokenPolicy {
    /// No tokens at all. Safe for any endpoint, including a third-party LLM.
    pub fn none() -> Self {
        Self {
            allowed: BTreeSet::new(),
            all: false,
        }
    }

    /// Every token, including literal values. LOCAL ENDPOINTS ONLY — this can carry
    /// secrets, PII and proprietary logic verbatim.
    pub fn all() -> Self {
        Self {
            allowed: BTreeSet::new(),
            all: true,
        }
    }

    /// Names and types but never values: enough to say *what* changed without disclosing
    /// the data. The useful middle, and the one most reviews want.
    pub fn signatures() -> Self {
        Self::none()
            .allow(Category::Identifier)
            .allow(Category::TypeReference)
            .allow(Category::Import)
    }

    /// Permit tokens for one more category.
    pub fn allow(mut self, category: Category) -> Self {
        self.allowed.insert(category);
        self
    }

    /// Forbid tokens for a category, even under [`TokenPolicy::all`].
    ///
    /// Lets a user say "everything except literals" without enumerating the rest — the
    /// common shape of a real policy.
    pub fn deny(mut self, category: Category) -> Self {
        self.allowed.remove(&category);
        if self.all {
            // Downgrade from blanket-allow to an explicit set minus this one, so the denial
            // cannot be silently overridden by the blanket flag.
            self.all = false;
            for c in Category::ALL {
                if c != category {
                    self.allowed.insert(c);
                }
            }
        }
        self
    }

    pub fn permits(&self, category: Category) -> bool {
        self.all || self.allowed.contains(&category)
    }

    pub fn is_none(&self) -> bool {
        !self.all && self.allowed.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_discloses_nothing() {
        // Fail closed: an unconfigured policy must never leak.
        let p = TokenPolicy::default();
        assert!(p.is_none());
        for c in Category::ALL {
            assert!(!p.permits(c), "{c:?} must not be permitted by default");
        }
    }

    #[test]
    fn signatures_carry_names_but_never_values() {
        let p = TokenPolicy::signatures();
        assert!(p.permits(Category::Identifier));
        assert!(p.permits(Category::TypeReference));
        // The whole point of the middle tier: structure and names, no data.
        assert!(!p.permits(Category::Literal));
    }

    #[test]
    fn deny_survives_a_blanket_allow() {
        // "Everything except literals" is the common real policy, and a denial that a
        // blanket flag could override would be worse than no denial at all.
        let p = TokenPolicy::all().deny(Category::Literal);
        assert!(!p.permits(Category::Literal));
        assert!(p.permits(Category::Identifier));
    }

    #[test]
    fn selective_allow_is_additive() {
        let p = TokenPolicy::none().allow(Category::Import);
        assert!(p.permits(Category::Import));
        assert!(!p.permits(Category::Identifier));
    }
}
