//! What a service method can refuse to do, and who asked it to.

use thiserror::Error;
use vesica_core::{SchemaError, ValidationError};
use vesica_store::StoreError;

/// The analyst working at the keyboard. Every audit entry that is not
/// attributed to a named agent is attributed to this.
///
/// It is a constant rather than a literal sprinkled through the code so that
/// a deployment which one day distinguishes individual analysts changes one
/// line — and so that `04-data-model.md`'s `'human' | agent name` convention
/// has exactly one definition.
pub const ANALYST: &str = "human";

/// Longest an actor name may be. Long enough for any plausible agent
/// identifier, short enough that the audit trail cannot be used as storage.
const MAX_ACTOR_LEN: usize = 64;

/// Who is performing an operation, as recorded in `audit_log.actor`.
///
/// Defaults to the analyst ([`ANALYST`]); a caller that knows better — the
/// MCP server acting for a named agent, an importer, a scheduled job — says
/// so with [`Actor::named`]. There is deliberately no way to build one from
/// an empty string: an audit entry that does not say who did it is worse
/// than no audit entry, because it looks like one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor(String);

impl Actor {
    /// The analyst at the keyboard — the default actor.
    pub fn analyst() -> Self {
        Actor(ANALYST.to_string())
    }

    /// A named actor: an agent, an importer, a job. Trimmed; rejected if it
    /// is empty or absurdly long.
    pub fn named(name: &str) -> Result<Self, ServiceError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(ServiceError::validation(
                "actor",
                "an actor name cannot be empty",
                format!(
                    "send a short name identifying who is acting, e.g. `{ANALYST}` \
                     or the agent's id, or omit it to default to `{ANALYST}`"
                ),
            ));
        }
        if trimmed.chars().count() > MAX_ACTOR_LEN {
            return Err(ServiceError::validation(
                "actor",
                format!("an actor name may be at most {MAX_ACTOR_LEN} characters"),
                format!("shorten the name to {MAX_ACTOR_LEN} characters or fewer"),
            ));
        }
        Ok(Actor(trimmed.to_string()))
    }

    /// What a transport does with the actor field it may or may not have
    /// been given: `None` is the analyst, `Some` is validated.
    pub fn from_optional(name: Option<&str>) -> Result<Self, ServiceError> {
        match name {
            None => Ok(Actor::analyst()),
            Some(name) => Actor::named(name),
        }
    }

    /// The string written to `audit_log.actor`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Actor {
    fn default() -> Self {
        Actor::analyst()
    }
}

/// Everything a service method can refuse to do.
///
/// Every variant that blames the caller carries a `fix`: a sentence saying
/// what a valid request looks like, written for an agent that will read it
/// and correct itself without asking a human (AGENT-BRIEF.md, "Style").
#[derive(Debug, Error)]
pub enum ServiceError {
    /// No such thing, or it is soft-deleted and therefore invisible to
    /// every read.
    #[error("no {kind} matching `{reference}`")]
    NotFound {
        /// What kind of thing was looked for, e.g. `case`.
        kind: &'static str,
        /// The id or slug that was looked for.
        reference: String,
    },

    /// The request was understood and refused.
    #[error("{message}")]
    Validation {
        /// The field at fault, as the caller named it.
        field: String,
        /// What is wrong with it.
        message: String,
        /// What a valid value looks like.
        fix: String,
    },

    /// The request cannot be satisfied given what is already stored.
    #[error("{message}")]
    Conflict {
        /// What clashed.
        message: String,
        /// What to do about it.
        fix: String,
    },

    /// A destructive operation was called without explicit confirmation.
    /// Separate from [`ServiceError::Validation`] because a transport
    /// should render it as "are you sure", not as "you typed it wrong".
    #[error("{operation} destroys data permanently and was not confirmed")]
    ConfirmationRequired {
        /// The operation that refused.
        operation: &'static str,
        /// How to confirm it.
        fix: String,
    },

    /// A stored row does not satisfy an invariant this code requires — a
    /// `kase.status` that is neither `active` nor `archived`, say. Not the
    /// caller's fault and not fixable by them; it means something wrote to
    /// the database around the service layer.
    #[error("stored data is inconsistent: {0}")]
    Inconsistent(String),

    /// The store failed.
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl ServiceError {
    pub(crate) fn validation(
        field: &str,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        ServiceError::Validation {
            field: field.to_string(),
            message: message.into(),
            fix: fix.into(),
        }
    }

    pub(crate) fn not_found(kind: &'static str, reference: impl Into<String>) -> Self {
        ServiceError::NotFound {
            kind,
            reference: reference.into(),
        }
    }

    /// Turns a `vesica-core` validation failure into a service one, keeping
    /// the `fix` string the value layer already wrote — it knows better
    /// than this layer what a valid IBAN or country code looks like.
    pub(crate) fn from_core(field: &str, e: ValidationError) -> Self {
        ServiceError::Validation {
            field: field.to_string(),
            message: e.message,
            fix: e.fix,
        }
    }

    /// Turns a schema-registry refusal — an unknown schema id, an attempt
    /// to instantiate an abstract schema — into a service validation error,
    /// keeping the `fix` the registry already wrote: it knows the
    /// catalogue, and this layer must never hardcode one.
    pub(crate) fn from_schema(field: &str, e: SchemaError) -> Self {
        ServiceError::Validation {
            field: field.to_string(),
            message: e.message,
            fix: e.fix,
        }
    }

    /// The machine-actionable remedy, where there is one. A transport
    /// surfaces this verbatim so an agent can correct itself.
    pub fn fix(&self) -> Option<&str> {
        match self {
            ServiceError::Validation { fix, .. }
            | ServiceError::Conflict { fix, .. }
            | ServiceError::ConfirmationRequired { fix, .. } => Some(fix),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_actor_is_the_analyst_and_a_caller_can_override_it() {
        assert_eq!(Actor::default().as_str(), ANALYST);
        assert_eq!(Actor::from_optional(None).unwrap().as_str(), ANALYST);
        assert_eq!(
            Actor::from_optional(Some("agent:researcher"))
                .unwrap()
                .as_str(),
            "agent:researcher"
        );
        assert_eq!(Actor::named("  padded  ").unwrap().as_str(), "padded");
    }

    #[test]
    fn an_actor_name_that_says_nothing_is_refused_with_a_fix() {
        let e = Actor::named("   ").unwrap_err();
        assert!(matches!(e, ServiceError::Validation { .. }));
        assert!(e.fix().unwrap().contains(ANALYST));

        let e = Actor::named(&"x".repeat(MAX_ACTOR_LEN + 1)).unwrap_err();
        assert!(e.fix().is_some());
    }
}
