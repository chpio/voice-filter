//! The translations in `i18n/`: they parse, have every text the code asks for and nothing else,
//! and the same texts in every language.

use ::std::collections::{BTreeSet, HashMap};
use ::std::path::Path;

use ::voice_filter::chain;
use ::voice_filter::i18n::{self, FALLBACK, LOCALES};

/// The ids of a translation.
fn ids(ftl: &str) -> BTreeSet<String> {
    ftl.lines()
        .filter_map(|line| {
            let (id, _) = line.split_once(" =")?;
            let starts = id.chars().next()?.is_ascii_lowercase();
            (starts && !id.contains(' ')).then(|| id.to_owned())
        })
        .collect()
}

fn english() -> &'static str {
    LOCALES
        .iter()
        .find(|(language, _)| *language == FALLBACK)
        .map(|(_, ftl)| *ftl)
        .unwrap()
}

/// The Rust sources of the crate, concatenated.
fn sources() -> String {
    fn walk(dir: &Path, out: &mut String) {
        for entry in ::std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push_str(&::std::fs::read_to_string(&path).unwrap());
            }
        }
    }
    let mut out = String::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut out);
    out
}

/// The ids the code builds from the names of the stages and parameters, with the English title
/// the daemon has for a parameter.
fn dynamic_ids() -> HashMap<String, Option<String>> {
    let (chain, _) = chain::build(None).unwrap();
    let mut ids = HashMap::new();
    for stage in chain.stages() {
        ids.insert(format!("stage-{}", stage.name()), None);
        ids.insert(format!("help-stage-{}", stage.name()), None);
    }
    for (stage, param) in chain.params() {
        let title = Some(param.title.to_owned());
        ids.insert(format!("param-{stage}-{}", param.name), title);
        ids.insert(format!("help-param-{stage}-{}", param.name), None);
    }
    let volume = ::voice_filter::volume::param();
    ids.insert(
        format!("param-listen-{}", volume.name),
        Some(volume.title.to_owned()),
    );
    ids
}

/// The ids given literally to `tr` and `tr_args`.
fn named(sources: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for call in ["tr(", "tr_args("] {
        for (at, _) in sources.match_indices(call) {
            // not the end of another name, e.g. `str(`
            let before = sources[..at].chars().next_back();
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let rest = sources[at + call.len()..]
                .trim_start()
                .trim_start_matches('&');
            if let Some(rest) = rest.strip_prefix('"') {
                ids.push(rest[..rest.find('"').unwrap()].to_owned());
            }
        }
    }
    ids
}

#[test]
fn every_translation_parses() {
    for (language, ftl) in LOCALES {
        if let Err(err) = i18n::bundle(language, ftl) {
            panic!("{err}");
        }
    }
}

#[test]
fn every_language_has_the_same_texts() {
    let english = ids(english());
    for (language, ftl) in LOCALES {
        let ids = ids(ftl);
        let missing: Vec<_> = english.difference(&ids).collect();
        let extra: Vec<_> = ids.difference(&english).collect();
        assert!(missing.is_empty(), "{language} lacks {missing:?}");
        assert!(
            extra.is_empty(),
            "{language} has {extra:?}, English doesn't"
        );
    }
}

/// Every id the code names literally is in the English file, every id there is named by the code
/// or built from a stage or parameter, and the titles of the parameters are the daemon's.
#[test]
fn the_code_and_the_english_texts_match() {
    let english = ids(english());
    let sources = sources();
    for id in named(&sources) {
        assert!(english.contains(&id), "no text {id} in i18n/en.ftl");
    }
    let dynamic = dynamic_ids();
    for id in &english {
        let literal = sources.contains(&format!("\"{id}\""));
        // the groups by the id in the window's list
        let group = id
            .strip_prefix("group-")
            .map(|group| group.trim_end_matches("-description"))
            .is_some_and(|group| sources.contains(&format!("(\"{group}\", &[")));
        assert!(
            literal || group || dynamic.contains_key(id),
            "{id} in i18n/en.ftl isn't used"
        );
    }
    for (id, title) in &dynamic {
        assert!(english.contains(id), "no text {id} in i18n/en.ftl");
        if let Some(title) = title {
            assert_eq!(&i18n::tr(id), title, "{id} differs from the daemon's title");
        }
    }
}

#[test]
fn reads_the_languages_from_the_environment() {
    let env = |pairs: &'static [(&str, &str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        }
    };
    assert_eq!(i18n::requested(env(&[("LANG", "de_DE.UTF-8")])), ["de-DE"]);
    assert_eq!(
        i18n::requested(env(&[("LANG", "de_DE.UTF-8"), ("LC_MESSAGES", "fr_FR")])),
        ["fr-FR"]
    );
    assert_eq!(
        i18n::requested(env(&[("LANGUAGE", "es:fr"), ("LANG", "de_DE.UTF-8")])),
        ["es", "fr", "de-DE"]
    );
    // the C locale means English, whatever LANGUAGE says
    assert!(i18n::requested(env(&[("LANGUAGE", "de"), ("LC_ALL", "C")])).is_empty());
    assert_eq!(
        i18n::requested(env(&[("LANG", "ca_ES@valencia")])),
        ["ca-ES"]
    );
}

#[test]
fn falls_back_to_english() {
    assert_eq!(i18n::negotiate(&[]), "en");
    assert_eq!(i18n::negotiate(&["xx-YY".to_owned()]), "en");
    assert_eq!(i18n::negotiate(&["en-US".to_owned()]), "en");
    assert_eq!(i18n::negotiate(&["de-DE".to_owned()]), "de");
    assert_eq!(
        i18n::negotiate(&["de-AT".to_owned(), "en".to_owned()]),
        "de"
    );
    // a missing text shows its id
    assert_eq!(i18n::tr("no-such-text"), "no-such-text");
}

#[test]
fn joins_lines_and_keeps_paragraphs() {
    assert_eq!(
        i18n::paragraphs("One line\n  goes on.\n\nA second."),
        "One line goes on.\n\nA second."
    );
}
