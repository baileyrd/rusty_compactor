//! Word/phrase-level rewrite tables used by [`crate::engine::compress_prose`].
//! Every entry is deliberately conservative: negations, modal-necessity words
//! ("must", "cannot", "without"), numbers, and anything already protected by
//! [`crate::segment`] are never touched, so compression cannot invert meaning.

/// Phrases deleted or shortened at every level. `(pattern, replacement)`;
/// patterns are matched case-insensitively as whole phrases.
pub const FILLER_PHRASES: &[(&str, &str)] = &[
    ("please note that", ""),
    ("it should be noted that", ""),
    ("it is worth noting that", ""),
    ("i would like to point out that", ""),
    ("i think that", ""),
    ("i believe that", ""),
    ("in my opinion", ""),
    ("due to the fact that", "because"),
    ("in spite of the fact that", "although"),
    ("in the event that", "if"),
    ("at this point in time", "now"),
    ("in order to", "to"),
    ("with regard to", "about"),
    ("with regards to", "about"),
    ("in regard to", "about"),
    ("for the purpose of", "to"),
    ("a large number of", "many"),
    ("a majority of", "most"),
    ("in the process of", ""),
    ("as a matter of fact", ""),
    ("basically", ""),
    ("essentially", ""),
    ("actually", ""),
    ("really", ""),
    ("simply", ""),
    ("just", ""),
    ("kind of", ""),
    ("sort of", ""),
    ("more or less", ""),
    ("i would like to", "I'll"),
    ("we would like to", "we'll"),
];

/// Standard contraction pairs, applied from `Full` level onward.
pub const CONTRACTIONS: &[(&str, &str)] = &[
    ("do not", "don't"),
    ("does not", "doesn't"),
    ("did not", "didn't"),
    ("is not", "isn't"),
    ("are not", "aren't"),
    ("was not", "wasn't"),
    ("were not", "weren't"),
    ("cannot", "can't"),
    ("will not", "won't"),
    ("have not", "haven't"),
    ("has not", "hasn't"),
    ("had not", "hadn't"),
    ("would not", "wouldn't"),
    ("could not", "couldn't"),
    ("should not", "shouldn't"),
    ("i am", "I'm"),
    ("i will", "I'll"),
    ("i have", "I've"),
    ("you are", "you're"),
    ("it is", "it's"),
    ("that is", "that's"),
    ("there is", "there's"),
    ("we are", "we're"),
    ("they are", "they're"),
    ("let us", "let's"),
];

/// Discourse markers dropped from sentence starts at `Ultra` level onward.
pub const DISCOURSE_MARKERS: &[&str] = &[
    "however,",
    "additionally,",
    "furthermore,",
    "moreover,",
    "in conclusion,",
    "in summary,",
    "as a result,",
    "that said,",
    "with that said,",
    "so,",
    "well,",
    "now,",
];

/// Dev-jargon abbreviations, applied at `Ultra` level onward. Whole-word,
/// case-insensitive; each maps singular/plural forms explicitly to keep
/// matching simple and predictable.
pub const ABBREVIATIONS: &[(&str, &str)] = &[
    ("information", "info"),
    ("configuration", "config"),
    ("configurations", "configs"),
    ("repository", "repo"),
    ("repositories", "repos"),
    ("documentation", "docs"),
    ("approximately", "~"),
    ("implementation", "impl"),
    ("implementations", "impls"),
    ("reference", "ref"),
    ("references", "refs"),
    ("requirement", "req"),
    ("requirements", "reqs"),
    ("environment", "env"),
    ("environments", "envs"),
    ("application", "app"),
    ("applications", "apps"),
    ("directory", "dir"),
    ("directories", "dirs"),
    ("argument", "arg"),
    ("arguments", "args"),
    ("parameter", "param"),
    ("parameters", "params"),
    ("dependency", "dep"),
    ("dependencies", "deps"),
    ("database", "db"),
];

/// Wordy connectors collapsed at `Wenyan` level.
pub const WENYAN_CONNECTORS: &[(&str, &str)] = &[
    ("for example,", "e.g."),
    ("for example", "e.g."),
    ("that is to say,", "i.e."),
    ("that is to say", "i.e."),
    ("as well as", "and"),
    ("in addition to", "and"),
    ("and so on", "etc."),
    ("and so forth", "etc."),
];

/// Words that must never be deleted by any rule, regardless of level, because
/// dropping them can invert meaning. Enforced by `engine::compress_prose`
/// skipping article/marker removal immediately after one of these.
pub const NEVER_DROP: &[&str] = &[
    "not", "no", "never", "without", "except", "unless", "must", "required", "cannot", "won't",
    "don't", "isn't", "aren't",
];
