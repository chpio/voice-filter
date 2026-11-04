//! The texts of the app and the tray in the user's language, from the Fluent files in `i18n/`
//! (one per language, embedded), English where a language lacks one. The messages of the command
//! line and the log stay English.
//!
//! In the files a single line break is a space, so the texts can be wrapped there, an empty line
//! starts a new paragraph.

use ::std::sync::OnceLock;

pub use ::fluent_bundle::FluentValue;
use ::fluent_bundle::concurrent::FluentBundle;
use ::fluent_bundle::{FluentArgs, FluentResource};
use ::fluent_langneg::{NegotiationStrategy, negotiate_languages};
use ::unic_langid::LanguageIdentifier;

/// The language every text is written in first, and the fallback.
pub const FALLBACK: &str = "en";

/// The translations, by language: add a line for a new one.
pub const LOCALES: [(&str, &str); 3] = [
    ("en", include_str!("../i18n/en.ftl")),
    ("de", include_str!("../i18n/de.ftl")),
    ("es", include_str!("../i18n/es.ftl")),
];

struct Texts {
    /// The negotiated language first, then English.
    bundles: Vec<FluentBundle<FluentResource>>,
}

static TEXTS: OnceLock<Texts> = OnceLock::new();

/// Parses a translation, failing on any error in it.
pub fn bundle(language: &str, ftl: &str) -> Result<FluentBundle<FluentResource>, String> {
    let id: LanguageIdentifier = language
        .parse()
        .map_err(|err| format!("{language}: {err}"))?;
    let resource = FluentResource::try_new(ftl.to_owned())
        .map_err(|(_, errors)| format!("{language}: {errors:?}"))?;
    let mut bundle = FluentBundle::new_concurrent(vec![id]);
    // GTK shows the isolation marks around placeables, and copying a value would copy them
    bundle.set_use_isolating(false);
    bundle
        .add_resource(resource)
        .map_err(|errors| format!("{language}: {errors:?}"))?;
    Ok(bundle)
}

/// The languages the user asks for, best first: `LANGUAGE` (a list, as gettext reads it, unless
/// the locale is `C`), then `LC_ALL`, `LC_MESSAGES` and `LANG`. `de_DE.UTF-8` becomes `de-DE`.
pub fn requested(var: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .into_iter()
        .find_map(|name| var(name).filter(|value| !value.is_empty()));
    let mut languages = Vec::new();
    if locale.as_deref().is_none_or(|locale| !is_c(locale))
        && let Some(list) = var("LANGUAGE")
    {
        languages.extend(list.split(':').map(str::to_owned));
    }
    languages.extend(locale);
    languages
        .into_iter()
        .filter(|language| !language.is_empty() && !is_c(language))
        .map(|language| {
            // drop the encoding and the modifier, `_` to `-`
            let language = language.split(['.', '@']).next().unwrap_or_default();
            language.replace('_', "-")
        })
        .collect()
}

fn is_c(locale: &str) -> bool {
    matches!(locale.split('.').next(), Some("C" | "POSIX"))
}

/// The language of [`LOCALES`] that fits `requested` best, English if none does.
pub fn negotiate(requested: &[String]) -> &'static str {
    let requested: Vec<LanguageIdentifier> = requested
        .iter()
        .filter_map(|language| language.parse().ok())
        .collect();
    let available: Vec<LanguageIdentifier> = LOCALES
        .iter()
        .map(|(language, _)| language.parse().expect("a valid language"))
        .collect();
    let fallback: LanguageIdentifier = FALLBACK.parse().expect("a valid language");
    let chosen = negotiate_languages(
        &requested,
        &available,
        Some(&fallback),
        NegotiationStrategy::Lookup,
    );
    let chosen = chosen
        .first()
        .map_or(FALLBACK.to_owned(), ToString::to_string);
    LOCALES
        .iter()
        .find(|(language, _)| *language == chosen)
        .map_or(FALLBACK, |(language, _)| language)
}

fn texts() -> &'static Texts {
    TEXTS.get_or_init(|| {
        let language = negotiate(&requested(|name| ::std::env::var(name).ok()));
        let bundles = [language, FALLBACK]
            .into_iter()
            .enumerate()
            .filter(|&(i, chosen)| i == 0 || chosen != language)
            .filter_map(|(_, chosen)| {
                let (_, ftl) = LOCALES.iter().find(|(lang, _)| *lang == chosen)?;
                bundle(chosen, ftl)
                    .inspect_err(|err| eprintln!("Skipped the translation {err}"))
                    .ok()
            })
            .collect();
        Texts { bundles }
    })
}

/// The text `id`, with `args` (name and value) for its placeables; `None` if no language has it.
/// Values formatted by the caller go in as strings, counts as numbers, for the plural forms.
pub fn lookup(id: &str, args: &[(&str, FluentValue<'_>)]) -> Option<String> {
    let args = (!args.is_empty()).then(|| {
        let mut fluent = FluentArgs::new();
        for (name, value) in args {
            fluent.set(*name, value.clone());
        }
        fluent
    });
    texts().bundles.iter().find_map(|bundle| {
        let pattern = bundle.get_message(id)?.value()?;
        let mut errors = Vec::new();
        let text = bundle.format_pattern(pattern, args.as_ref(), &mut errors);
        Some(paragraphs(&text))
    })
}

/// The text `id`; the id itself if it's missing, so a missing text shows instead of nothing.
pub fn tr(id: &str) -> String {
    lookup(id, &[]).unwrap_or_else(|| id.to_owned())
}

/// [`tr`] with values for the placeables, by name (`{ $floor }` in the file).
pub fn tr_args(id: &str, args: &[(&str, FluentValue<'_>)]) -> String {
    lookup(id, args).unwrap_or_else(|| id.to_owned())
}

/// Single line breaks to spaces, empty lines to a paragraph break.
pub fn paragraphs(text: &str) -> String {
    text.split("\n\n")
        .map(|paragraph| {
            paragraph
                .split('\n')
                .map(str::trim)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
