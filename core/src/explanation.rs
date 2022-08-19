use crate::exchanges::common::{Amount, Price};
use mmb_database::impl_event;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};

pub struct Reason(Option<String>);

impl From<String> for Reason {
    fn from(value: String) -> Self {
        Reason(Some(value))
    }
}

impl From<&str> for Reason {
    fn from(value: &str) -> Self {
        Reason(Some(value.to_string()))
    }
}

impl From<Option<String>> for Reason {
    fn from(value: Option<String>) -> Self {
        Reason(value)
    }
}

impl From<Option<&str>> for Reason {
    fn from(value: Option<&str>) -> Self {
        Reason(value.map(|x| x.to_string()))
    }
}

#[derive(Debug, Default, Clone)]
pub struct Explanation {
    reasons: Vec<String>,
}

impl Explanation {
    pub(crate) fn get_reasons(&self) -> Vec<String> {
        self.reasons.clone()
    }
}

impl Explanation {
    pub fn add_reason(&mut self, reason: impl Into<Reason>) {
        let reason = reason.into();
        if let Reason(Some(reason)) = reason {
            self.reasons.push(reason);
        }
    }

    #[cfg(test)]
    fn reasons(self) -> Vec<String> {
        self.reasons
    }
}

pub struct WithExplanation<T> {
    pub value: T,
    pub explanation: Explanation,
}

impl<T> WithExplanation<T> {
    pub(crate) fn as_mut_all(&mut self) -> (&mut T, &mut Explanation) {
        (&mut self.value, &mut self.explanation)
    }
}

impl<T: Debug> Debug for WithExplanation<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WithExplanation")
            .field("value", &self.value)
            .field("explanation", &self.explanation)
            .finish()
    }
}

impl<T: Default> Default for WithExplanation<T> {
    fn default() -> Self {
        WithExplanation {
            value: Default::default(),
            explanation: Default::default(),
        }
    }
}

impl<T: Clone> Clone for WithExplanation<T> {
    fn clone(&self) -> Self {
        WithExplanation {
            value: self.value.clone(),
            explanation: self.explanation.clone(),
        }
    }
}

impl<T: PartialEq> PartialEq for WithExplanation<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq + PartialEq> Eq for WithExplanation<T> {}

pub trait OptionExplanationAddReasonExt {
    fn add_reason(&mut self, reason: String);

    fn with_reason<C>(&mut self, f: impl FnOnce() -> C)
    where
        C: Into<Reason>;
}

impl OptionExplanationAddReasonExt for Option<Explanation> {
    fn add_reason(&mut self, reason: String) {
        if let Some(explanation) = self {
            explanation.add_reason(reason);
        }
    }

    fn with_reason<C>(&mut self, reason: impl FnOnce() -> C)
    where
        C: Into<Reason>,
    {
        if let Some(explanation) = self {
            explanation.add_reason(reason());
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevelExplanation {
    pub mode_name: String,
    pub price: Price,
    pub amount: Amount,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ExplanationSet {
    set: Vec<PriceLevelExplanation>,
}

impl ExplanationSet {
    pub fn new(set: Vec<PriceLevelExplanation>) -> Self {
        Self { set }
    }
}

impl_event!(ExplanationSet, "disposition_explanations");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    pub fn add_reason() {
        let mut explanation = Explanation::default();

        explanation.add_reason("test");

        let expected = vec!["test".to_string()];
        assert_eq!(explanation.reasons(), expected);
    }
}
