# UI redesign, PR 1: Foundations — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lay the groundwork the new screens will be built on:
- the `.resx` translation system (English and French);
- engine messages turned into translatable codes;
- the tweak catalog texts moved into the `.resx` files;
- the System, Light and Dark themes with design tokens;
- runtime loading of the Windows fonts and icons;
- the `ui::kit` component layer.

When this PR lands, the app still works with its current screens. Nothing
visible changes yet, apart from fonts and colors.

**Architecture:**
- **Translations.** `src/i18n/` parses the `.resx` files embedded by
  `build.rs` once. A copyable `Tr` handle translates keys, falling back to
  English.
- **Engine messages.** The engines return enums (`RuleProblem`,
  `DriftReason`, `CustomReason`). Each enum's `Display` gives the exact
  English sentence used today, so existing reports and history are
  unchanged. The UI translates the enums through `i18n::messages`.
- **Theme.** `ui::kit` owns the tokens, the egui styles for both themes,
  font and icon loading, and the components. The theme preference goes
  through egui 0.29's own `ThemePreference`, so following the Windows
  theme comes from eframe.

**Tech stack:** Rust 1.88, egui/eframe 0.29.1, quick-xml 0.41 (already in
`Cargo.lock`, pulled in by another crate), ab_glyph 0.2 (already pulled in
by epaint), winreg 0.52.

**Spec:** `docs/superpowers/specs/2026-11-23-ui-redesign-design.md`. Read
§3, §6.1, §6.2 and §6.4. This plan is PR 1 of §7. PRs 2 to 6 get their own
plan, each written once the previous PR is merged, because they build on
the APIs this one fixes.

## Global constraints

- `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`
  and `cargo test --locked` stay green on every commit.
- The engine logic doesn't change: the same conditions produce the same
  problems, reasons and statuses. Only the type that carries them changes.
  An engine test may move its assertion from a sentence to the matching
  code, but must not lose what it checks.
- Fonts are **loaded at runtime from the Windows fonts folder, never
  bundled**:
  - text: `SegUIVar.ttf`, falling back to `segoeui.ttf`;
  - semibold: `seguisb.ttf`;
  - icons: `SegoeIcons.ttf`, falling back to `segmdl2.ttf`.

  If a file is missing or unreadable, the app falls back to the next file
  or to egui's default font. It must never crash.
- Tests stay headless (`ctx.run`). No test may depend on system fonts
  being present. A test that uses the real fonts folder skips itself when
  that folder is missing.
- Colors:
  - accent `#0078D4` in light; a `#4CC2FF`-like tone in dark;
  - corners 4 px on controls, 8 px on cards; 1 px hairline borders;
  - spacing on a 4 px scale: 4, 8, 12, 16, 24, 32.
- Type scale: page title 24, section 15 semibold, body 14, secondary 12.
- Translations:
  - English (`i18n/strings.resx`) is the reference and the fallback;
  - a key missing even in English renders as the key itself, never a panic;
  - placeholders are .NET-style `{0}`, `{1}`, with `{{` and `}}` as
    literal braces.
- `config.json` gains only `language` (default `"en"` when absent). Older
  configs keep loading.
- One branch for this PR: `feat/ui-foundations`.

## Review focus

These inputs aren't covered by the spec's examples but would hurt a real
user. Each one is pinned by a test in the task that owns it.

1. **A corrupt or truncated font file** in the Windows fonts folder:
   egui panics on font data it can't parse. The file must be skipped, and
   the fallback used (Task 6).
2. **A translation whose placeholders don't match English**: a missing
   `{1}`, an extra `{2}`, or a stray `{` in French text. It must never
   panic or leave a raw `{1}` on screen. The shipped files are checked by
   a test (Task 1).
3. **An older or edited `config.json`**: no `language`, a language that
   isn't shipped (`"de"`), or `theme: "Dark"` from v0.2. It must load,
   fall back to English, and keep the chosen theme (Task 5).
4. **Windows switching between light and dark while WinTune is open**:
   the palette must follow on the next frame, without a restart (Task 4).
5. **A `.resx` value with XML entities, line breaks or a `<comment>`**
   (`&amp;`, `&lt;`, multi-line text): it must be decoded exactly, with the
   comment ignored (Task 1).

---

### Task 1: Translation core (`src/i18n/`)

**Files:**
- Create: `i18n/strings.resx`, `i18n/strings.fr.resx`, `src/i18n/mod.rs`,
  `src/i18n/resx.rs`, `src/i18n/tests.rs`
- Modify: `build.rs` (generates the language table), `Cargo.toml`
  (`quick-xml = "0.41"`), `src/lib.rs` (`pub mod i18n;`)

**Interfaces:**
- Produces:
  - `pub fn parse_resx(xml: &str) -> Result<BTreeMap<String, String>, String>`
    in `src/i18n/resx.rs`.
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct Tr` in
    `src/i18n/mod.rs`, with:
    - `pub fn new(code: &str) -> Tr` (an unknown code gives English);
    - `pub fn english() -> Tr`;
    - `pub fn code(&self) -> &'static str`;
    - `pub fn t(&self, key: &str) -> String`;
    - `pub fn t_args(&self, key: &str, args: &[&dyn std::fmt::Display]) -> String`;
    - `pub fn languages() -> Vec<(&'static str, String)>`: the code, and
      the language's own name from its `language.name` key, English first,
      then by code.
  - `pub(crate) fn keys_of(code: &str) -> Option<&'static BTreeMap<String, String>>`,
    for tests.
- `build.rs` writes `$OUT_DIR/i18n_langs.rs`, which contains:
  - `pub(crate) static LANG_FILES: &[(&str, &str)] = &[("en", include_str!(…strings.resx)), ("fr", include_str!(…strings.fr.resx))];`
  - entries found by scanning `i18n/`: `strings.resx` becomes `en`, and
    `strings.<code>.resx` becomes `<code>`;
  - `cargo:rerun-if-changed=i18n`.

  `src/i18n/mod.rs` brings it in with `include!`. Every catalog is parsed
  once, into a `static OnceLock`. A file that fails to parse panics in a
  test (shipped files must be valid) but is skipped at runtime, with a
  `debug_assert!`.

- [ ] **Step 1: Write the failing tests in `src/i18n/tests.rs`**

```rust
#[test]
fn resx_values_are_decoded_and_comments_ignored() {
    let xml = r#"<root><data name="a" xml:space="preserve"><value>Tom &amp; Jerry &lt;3</value><comment>ignored</comment></data>
<data name="b"><value>line 1
line 2</value></data></root>"#;
    let map = parse_resx(xml).unwrap();
    assert_eq!(map["a"], "Tom & Jerry <3");
    assert_eq!(map["b"], "line 1\nline 2");
    assert_eq!(map.len(), 2);
}

#[test]
fn malformed_resx_is_an_error_not_a_panic() {
    assert!(parse_resx("<root><data name=\"a\"><value>x</data>").is_err());
}

#[test]
fn a_key_missing_in_french_falls_back_to_english_and_unknown_keys_show_themselves() {
    // Uses a key that exists only in English in a test-only catalog: see
    // `Tr::with_catalogs` below.
    let tr = Tr::with_catalogs("fr", &[("en", "<root><data name=\"only.en\"><value>Hello</value></data></root>"), ("fr", "<root/>")]);
    assert_eq!(tr.t("only.en"), "Hello");
    assert_eq!(tr.t("no.such.key"), "no.such.key");
}

#[test]
fn placeholders_are_filled_and_never_panic() {
    let tr = Tr::with_catalogs("en", &[("en", r#"<root>
<data name="two"><value>{0} of {1}</value></data>
<data name="braces"><value>{{literal}} {0}</value></data>
<data name="extra"><value>{0} and {2}</value></data>
<data name="stray"><value>oops { here</value></data></root>"#)]);
    assert_eq!(tr.t_args("two", &[&3, &13]), "3 of 13");
    assert_eq!(tr.t_args("braces", &[&"x"]), "{literal} x");
    assert_eq!(tr.t_args("extra", &[&"a"]), "a and ");   // missing argument: empty
    assert_eq!(tr.t_args("stray", &[]), "oops { here");   // unmatched brace kept verbatim
}

#[test]
fn unknown_language_codes_give_english() {
    assert_eq!(Tr::new("de").code(), "en");
    assert_eq!(Tr::new("fr").code(), "fr");
}

#[test]
fn shipped_languages_are_listed_with_their_own_names() {
    let langs = Tr::languages();
    assert_eq!(langs[0], ("en", "English".to_string()));
    assert!(langs.contains(&("fr", "Français".to_string())));
}

#[test]
fn shipped_translations_have_no_orphan_keys() { /* every key of every language exists in en */ }

#[test]
fn shipped_translations_are_complete() { /* en and fr: every en key exists in fr */ }

#[test]
fn shipped_translations_use_the_same_placeholders_as_english() {
    /* for each fr value: the set of {n} indexes equals the en value's set */
}

#[test]
fn every_key_used_in_the_code_exists_in_english() {
    /* walk src/**/*.rs (skip src/i18n/), regex r#"\.t(?:_args)?\(\s*"([A-Za-z0-9_.]+)""#,
       assert each capture is in keys_of("en") — list all missing keys in the failure message */
}
```

`Tr::with_catalogs(code, &[(code, xml)]) -> Tr` is a
`#[cfg(test)] pub(crate)` constructor. It leaks the parsed maps
(`Box::leak`) so that `Tr` stays `Copy`. Test catalogs are small, so the
leak doesn't matter.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --lib i18n`
Expected: compile error (`parse_resx`, `Tr` are not defined).

- [ ] **Step 3: Implement**

  - **`parse_resx`.** Read events with `quick_xml::Reader`. Collect
    `<data name>` → `<value>` text, unescaped. Ignore `<comment>`,
    `<resheader>` and `<schema>`. Return `Err` on any reader error or
    unclosed tag.
  - **`Tr` lookup.** `Tr` holds the language code and two
    `&'static BTreeMap<String, String>` references, the language's and
    English's. A lookup tries the language's map, then English, then
    returns the key.
  - **`t_args` placeholders.**
    - `{n}` → `args[n]`, or empty when that argument is missing.
    - `{{` → `{`, `}}` → `}`.
    - Any other brace is copied verbatim.
  - **The two `.resx` files.** Use the standard ResX header
    (`resmimetype`, `version`, `reader`, `writer`). For now, each file
    holds only `language.name`: `English` / `Français`. Later tasks add
    their keys.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test --lib i18n`
Expected: all of the tests above PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock build.rs i18n src/i18n src/lib.rs
git commit -m "feat(i18n): resx translation core with English fallback"
```

### Task 2: Engine messages become codes

**Files:**
- Modify:
  - `src/associations/validate.rs`;
  - `src/associations/status.rs`;
  - `src/associations/mod.rs` (re-exports);
  - `src/associations/apply.rs:344` (`.map_err(|e| e.to_string())`);
  - `src/tweaks.rs:396-402` and `:505-525`;
  - `src/ui/tweaks_tab.rs:40`;
  - `src/ui/associations_tab.rs` and `src/ui/rule_status.rs` (call sites:
    `.to_string()` wherever a `String` was used).
- Create: `src/i18n/messages.rs`
- Test:
  - `src/associations/tests.rs`;
  - `src/tweaks.rs` (tests module);
  - `src/i18n/tests.rs`.

**Interfaces:**
- Produces (in `associations`, re-exported from `associations::mod`):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleProblem {
    InvalidExtension { ext: String },
    ReservedExtension { ext: String },
    InvalidProgId { prog_id: String },
    ReservedProgId { prog_id: String, suggestion: String }, // "Custom<EXT>File"
    DuplicateExtension { ext: String },
    SharedProgId { prog_id: String },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftField { Description, Icon, OpenCommand }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftReason {
    DefaultChanged { ext: String, found: String, expected: String },
    DefaultMissing { ext: String, expected: String },
    ProgIdMissing { prog_id: String },
    ValueDiffers { field: DriftField, expected: String, found: String },
    UserChoice { choice: String, user: String, ext: String, prog_id: String },
    UserOverride { user: String, ext: String, prog_id: String },
}
```

  - `validate_extension(cfg) -> Result<(), RuleProblem>`
  - `find_rule_problems(rules) -> Vec<(usize, RuleProblem)>`
  - `RuleStatus::Drifted(Vec<DriftReason>)`
- Produces (in `tweaks`):
  - `pub enum CustomReason { UnexpectedValue { name: String, value: u32 }, UnexpectedType { name: String }, Unreadable(String) }`
  - `TweakStatus::Custom(CustomReason)`
- Each enum's `impl Display` gives **exactly today's English sentence**.
  Copy the `format!` strings verbatim from `validate.rs`, `status.rs`,
  `tweaks.rs:511-514` and `tweaks_tab.rs:40`. For `DriftField`, the labels
  are `description`, `icon` and `open command`.
- Produces in `src/i18n/messages.rs`:
  - `pub fn rule_problem(tr: Tr, p: &RuleProblem) -> String`
  - `pub fn drift_reason(tr: Tr, r: &DriftReason) -> String`
  - `pub fn custom_reason(tr: Tr, c: &CustomReason) -> String`

  The keys are:
  - `problem.invalid_extension`, `problem.reserved_extension`,
    `problem.invalid_prog_id`, `problem.reserved_prog_id`,
    `problem.duplicate_extension`, `problem.shared_prog_id`;
  - `drift.default_changed`, `drift.default_missing`,
    `drift.prog_id_missing`, `drift.value_differs`,
    `drift.field.description`, `drift.field.icon`,
    `drift.field.open_command`, `drift.user_choice`, `drift.user_override`;
  - `custom.unexpected_value`, `custom.unexpected_type`,
    `custom.unreadable`.

  Arguments follow the order of the enum fields. All of these keys go into
  both `.resx` files.

- [ ] **Step 1: Write the failing tests**

In `src/associations/tests.rs`, change the helper
`reasons(status) -> Vec<String>` to
`r.iter().map(ToString::to_string).collect()`. Every existing drift test
then keeps checking the same sentence. Add these tests:

```rust
#[test]
fn drift_reasons_are_codes() {
    // same setup as the existing test where Windows switched .foo to another ProgID
    let status = /* … */;
    let RuleStatus::Drifted(r) = status else { panic!() };
    assert!(r.contains(&DriftReason::DefaultChanged { ext: ".foo".into(), found: "OtherFile".into(), expected: "CustomFooFile".into() }));
}

#[test]
fn rule_problems_are_codes() {
    let rules = [with_ext(".foo"), with_ext(".FOO")];
    assert_eq!(find_rule_problems(&rules)[0].1, RuleProblem::DuplicateExtension { ext: ".foo".into() });
    let mut c = foo_cfg("CLSID");
    assert_eq!(validate_extension(&c), Err(RuleProblem::ReservedProgId { prog_id: "CLSID".into(), suggestion: "CustomFOOFile".into() }));
    c.extension = ".exe".into();
    assert_eq!(validate_extension(&c), Err(RuleProblem::ReservedExtension { ext: ".exe".into() }));
}

#[test]
fn problem_display_keeps_the_english_sentences() {
    assert_eq!(RuleProblem::DuplicateExtension { ext: ".foo".into() }.to_string(), ".foo is listed more than once");
}
```

The existing tests at `tests.rs:590-615` and `:700-720` keep their
`.contains(...)` checks, called on `p.1.to_string()`.

In the `src/tweaks.rs` tests, add
`a_dword_with_an_unknown_value_is_custom_with_its_value`. It sets
`HideFileExt = 7` in the sandbox and asserts
`TweakStatus::Custom(CustomReason::UnexpectedValue { name: "HideFileExt".into(), value: 7 })`.

In `src/i18n/tests.rs`, add
`every_engine_message_translates_in_every_shipped_language`. It builds
one value of each variant, with sample strings, and for each language in
`Tr::languages()` asserts that the result:
- is not empty;
- does not equal the key;
- contains no `{` followed by a digit;
- contains each sample argument.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --lib`
Expected: compile errors (`DriftReason`, `RuleProblem`, `CustomReason`
are unknown).

- [ ] **Step 3: Implement**

Swap each `format!` for the matching variant, and add the `Display`
impls. Fix the call sites in `apply.rs`, `tweaks_tab.rs`,
`associations_tab.rs` and `rule_status.rs` with `.to_string()`; the old
screens keep showing English. Write `messages.rs` and its `.resx` entries.
French texts are full, natural sentences.

- [ ] **Step 4: Run the full suite**

Run: `cargo test --locked && cargo clippy --all-targets --locked -- -D warnings`
Expected: all tests PASS, the 161 existing ones included, with no
warnings.

- [ ] **Step 5: Commit**

```bash
git add src i18n
git commit -m "refactor: engine messages as codes, translated by the UI"
```

### Task 3: Tweak catalog texts move to `.resx`

**Files:**
- Modify:
  - `src/tweaks.rs`: remove `title` and `description` from `Tweak` and
    from the 19 `CATALOG` entries;
  - `src/tweaks.rs:625`: the warning becomes `"{id}: needs Windows 11"`;
  - `TweakGroup::title()` is removed and replaced by
    `pub fn id(self) -> &'static str`, which returns `file_explorer`,
    `taskbar`, `start` or `privacy`;
  - `src/ui/tweaks_tab.rs`: titles, descriptions, search and group
    headings go through `Tr`;
  - `i18n/*.resx`.
- Test: `src/i18n/tests.rs`, `src/tweaks.rs` tests

**Interfaces:**
- Consumes: `Tr` (Task 1). The app must have a `tr: Tr` field, which
  doesn't exist before Task 5. Until then, add the field here, set to
  `Tr::english()`; Task 5 builds it from the config.
- Produces, in `src/i18n/messages.rs`:
  - `pub fn tweak_title(tr: Tr, id: &str) -> String`, key
    `tweak.<id>.title`;
  - `pub fn tweak_description(tr: Tr, id: &str) -> String`, key
    `tweak.<id>.description`;
  - `pub fn group_title(tr: Tr, g: TweakGroup) -> String`, key
    `group.<id>`.
- The English values are the current strings, copied verbatim from
  `CATALOG` and `TweakGroup::title()`.

- [ ] **Step 1: Write the failing tests**

```rust
// src/i18n/tests.rs
#[test]
fn every_tweak_and_group_has_texts_in_every_shipped_language() {
    for (code, _) in Tr::languages() {
        let tr = Tr::new(code);
        for t in crate::tweaks::catalog() {
            assert_ne!(messages::tweak_title(tr, t.id), format!("tweak.{}.title", t.id), "{code}");
            assert_ne!(messages::tweak_description(tr, t.id), format!("tweak.{}.description", t.id), "{code}");
        }
        for g in crate::tweaks::TweakGroup::ALL {
            assert_ne!(messages::group_title(tr, g), format!("group.{}", g.id()), "{code}");
        }
    }
}

#[test]
fn english_tweak_texts_are_unchanged() {
    assert_eq!(messages::tweak_title(Tr::english(), "show_extensions"), "Show file extensions");
}
```

The existing UI search test in the tweaks tab, if there is one, must keep
passing with English.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --lib i18n`
Expected: FAIL (`tweak_title` is not defined).

- [ ] **Step 3: Implement**

Move the strings into the `.resx` files, translate them into French, and
switch `tweaks_tab.rs` to `messages::*` with `self.tr`.

- [ ] **Step 4: Run the full suite**

Run: `cargo test --locked`
Expected: PASS, including `the_registry_reference_lists_every_tweak`.

- [ ] **Step 5: Commit**

```bash
git commit -am "refactor(tweaks): catalog texts live in the resx files"
```

### Task 4: Tokens and themed styles (`src/ui/kit/`)

**Files:**
- Create:
  - `src/ui/kit/mod.rs`;
  - `src/ui/kit/tokens.rs`;
  - `src/ui/kit/style.rs`;
  - `src/ui/kit/tests.rs`.
- Modify:
  - `src/ui/mod.rs` (`pub mod kit;`);
  - `src/theme.rs`: `ThemeMode` gains `System` and becomes
    `#[default] System`. The rest of `theme.rs` stays until PR 6.

**Interfaces:**
- Produces:

```rust
// tokens.rs
pub struct Palette {
    pub page: Color32, pub surface: Color32, pub surface_hover: Color32, pub control: Color32,
    pub border: Color32, pub text: Color32, pub text_secondary: Color32, pub text_disabled: Color32,
    pub accent: Color32, pub accent_hover: Color32, pub on_accent: Color32,
    pub success: Color32, pub warning: Color32, pub danger: Color32, pub neutral: Color32,
}
pub fn palette(theme: egui::Theme) -> &'static Palette;
pub mod space { pub const XS: f32 = 4.0; pub const S: f32 = 8.0; pub const M: f32 = 12.0; pub const L: f32 = 16.0; pub const XL: f32 = 24.0; pub const XXL: f32 = 32.0; }
pub mod radius { pub const CONTROL: f32 = 4.0; pub const CARD: f32 = 8.0; }
pub mod text_size { pub const TITLE: f32 = 24.0; pub const SECTION: f32 = 15.0; pub const BODY: f32 = 14.0; pub const SECONDARY: f32 = 12.0; }
// style.rs
pub fn install_styles(ctx: &egui::Context);                        // set_style_of(Light/Dark, …) once
pub fn set_theme_mode(ctx: &egui::Context, mode: ThemeMode);       // System→ThemePreference::System, etc.
pub fn current_palette(ctx: &egui::Context) -> &'static Palette;   // palette(ctx.theme())
pub fn resolved(mode: ThemeMode, ctx: &egui::Context) -> ThemeMode; // System → Dark/Light from ctx.theme(); never returns System
```

- The old helpers in `theme.rs` (`get_palette`, `card_frame`, …) are only
  ever given `Dark` or `Light`. In `get_palette`, map `System` to the dark
  palette, with a comment saying it is never passed. The old screens pass
  `self.ui_theme`, which comes from `resolved`; Task 5 does that wiring.

- [ ] **Step 1: Write the failing tests in `src/ui/kit/tests.rs`**

```rust
#[test]
fn system_mode_follows_windows_between_frames() {
    let ctx = egui::Context::default();
    install_styles(&ctx);
    set_theme_mode(&ctx, ThemeMode::System);
    let mut input = RawInput { system_theme: Some(egui::Theme::Light), ..Default::default() };
    ctx.run(input.clone(), |_| {});
    assert_eq!(current_palette(&ctx).page, palette(egui::Theme::Light).page);
    input.system_theme = Some(egui::Theme::Dark);
    ctx.run(input, |_| {});
    assert_eq!(current_palette(&ctx).page, palette(egui::Theme::Dark).page);
}

#[test]
fn explicit_modes_ignore_windows() {
    /* Light mode with system_theme = Dark → light palette, and vice versa */
}

#[test]
fn palettes_meet_contrast_minimums() {
    // WCAG relative luminance: text on page and surface ≥ 4.5, text_secondary ≥ 4.5,
    // on_accent on accent ≥ 4.5, for both themes.
}

#[test]
fn accent_is_windows_blue_in_light() {
    assert_eq!(palette(egui::Theme::Light).accent, Color32::from_rgb(0x00, 0x78, 0xD4));
}

#[test]
fn old_configs_keep_their_theme() {
    let c: ThemeMode = serde_json::from_str("\"Dark\"").unwrap();
    assert_eq!(c, ThemeMode::Dark);
    assert_eq!(ThemeMode::default(), ThemeMode::System);
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --lib kit`
Expected: compile error (the `kit` module is missing).

- [ ] **Step 3: Implement**

Use colors in the spirit of Windows 11 Settings, within the §6.1 rules.
For example, light: page `#F3F3F3`, surface `#FFFFFF`; dark: page
`#202020`, surface `#2B2B2B`, accent `#4CC2FF` with `on_accent` black.
`install_styles` sets these on each theme's `Style`:
- spacing, from the tokens;
- `Rounding`: 4 px on widgets, 8 px on windows;
- 1 px border strokes;
- `selection` in the accent color;
- `TextStyle` sizes, from `text_size`.

`AppConfig::default()` gets `theme: ThemeMode::System`.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test --lib kit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ui/kit src/ui/mod.rs src/theme.rs src/models.rs
git commit -m "feat(kit): design tokens, light/dark styles, System theme"
```

### Task 5: Config language and app wiring

**Files:**
- Modify:
  - `src/models.rs`: `AppConfig` gains
    `#[serde(default = "default_language")] pub language: String`, with
    `"en"` as the default;
  - `src/ui/mod.rs`:
    - `tr: Tr` (the field Task 3 added) is now built from
      `config.language`, and there is a new field `ui_theme: ThemeMode`;
    - `with_config` calls `kit::install_styles` and
      `kit::set_theme_mode(ctx, config.theme)` instead of
      `apply_fluent_theme`;
    - `tr = Tr::new(&config.language)`;
    - `ui()` sets `self.ui_theme = resolved(...)` each frame;
  - every `self.config.theme` passed to an old helper in `src/ui/*.rs`
    becomes `self.ui_theme`;
  - the old theme `ComboBox`, wherever it is today, gains a "System"
    entry and calls `kit::set_theme_mode` when changed.
- Test: `src/ui/mod.rs` tests

**Interfaces:**
- Consumes: `Tr` (Task 1), `install_styles`, `set_theme_mode` and
  `resolved` (Task 4).
- Produces:
  - `WinTuneApp.tr: Tr`, which PR 2 reads everywhere;
  - `pub(super) fn set_language(&mut self, code: &str)`, which updates
    `config.language` and `tr`. PR 2's Settings page calls it.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn configs_without_language_load_as_english() {
    let c: AppConfig = serde_json::from_str(r#"{"theme":"Dark","code_executable":"","icon_source_folder":"","icon_destination_folder":"","pak_file_path":"","extensions":[]}"#).unwrap();
    assert_eq!(c.language, "en");
    assert_eq!(c.theme, ThemeMode::Dark);
}

#[test]
fn an_unshipped_language_renders_in_english() {
    let icons = TempIcons::new(&["foo"]);
    let ctx = egui::Context::default();
    let mut config = icons.config();
    config.language = "de".into();
    let app = icons.app_with(&ctx, config);
    assert_eq!(app.tr.code(), "en");
}

#[test]
fn set_language_switches_immediately() {
    /* app in en → set_language("fr") → app.tr.code() == "fr" and config.language == "fr";
       one frame renders without panic */
}

#[test]
fn the_app_renders_a_frame_in_both_themes() {
    /* config.theme = Light then Dark: a frame renders, and ui_theme matches */
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --lib ui::tests`
Expected: compile error (`language` and `tr` are unknown).

- [ ] **Step 3: Implement the wiring described in Files.**

- [ ] **Step 4: Run the full suite**

Run: `cargo test --locked && cargo clippy --all-targets --locked -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git commit -am "feat: language setting and System theme wired into the app"
```

### Task 6: Windows fonts and icons at runtime

**Files:**
- Create: `src/ui/kit/fonts.rs`, `src/ui/kit/icons.rs`
- Modify:
  - `Cargo.toml`: add `ab_glyph = "0.2"`, which is already in the lock;
  - `src/ui/mod.rs`: `WinTuneApp::new` calls `kit::fonts::install` with
    `%WINDIR%\Fonts`. Fall back to `C:\Windows\Fonts`. **Tests never
    call this path through `with_config`.**
- Test: `src/ui/kit/tests.rs`

**Interfaces:**
- Produces:

```rust
// fonts.rs
pub const SEMIBOLD: &str = "wintune-semibold";   // FontFamily::Name
pub const ICONS: &str = "wintune-icons";         // FontFamily::Name
#[derive(Debug, PartialEq, Eq)]
pub struct FontReport { pub text: Option<&'static str>, pub semibold: bool, pub icons: Option<&'static str> }
pub fn install(ctx: &egui::Context, fonts_dir: &Path) -> FontReport;  // file names that were loaded
pub fn semibold(size: f32) -> egui::FontId;      // falls back to Proportional when not loaded
// icons.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon { Home, Associations, Explorer, Start, ContextMenu, Icons, History, Settings,
                Search, Add, Delete, Restore, Warning, Info, Error, Success, ChevronRight, ChevronDown, More, Folder }
pub fn glyph(icon: Icon) -> char;                // code points valid in both SegoeIcons and MDL2
pub fn icons_available(ctx: &egui::Context) -> bool;   // from ctx data, set by install
pub fn icon_text(ctx: &egui::Context, icon: Icon, size: f32) -> Option<egui::RichText>;  // None without an icon font
```

- How `install` works:
  - Each candidate file is read and checked with
    `ab_glyph::FontRef::try_from_slice`. A file that fails the check is
    skipped.
  - Text font: `SegUIVar.ttf`, then `segoeui.ttf`. It is inserted first in
    `Proportional`, with egui's defaults kept after it as a fallback for
    missing glyphs.
  - `seguisb.ttf` becomes the `SEMIBOLD` family.
  - Icons: `SegoeIcons.ttf`, then `segmdl2.ttf`, as the `ICONS` family.
  - `icons_available` is stored with
    `ctx.data_mut(|d| d.insert_temp(Id::new("wintune.icons"), bool))`.
- Glyphs use the shared Fluent/MDL2 code points: Home `E80F`, Settings
  `E713`, Search `E721`, Add `E710`, Delete `E74D`, History `E81C`, Warning
  `E7BA`, Info `E946`, More `E712`, ChevronRight `E76C`, ChevronDown `E70D`,
  Folder `E8B7`. The remaining glyphs are the implementer's choice, and
  must exist in both fonts.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn missing_fonts_folder_falls_back_without_panicking() {
    let ctx = egui::Context::default();
    let r = fonts::install(&ctx, Path::new(r"C:\definitely\not\here"));
    assert_eq!(r, FontReport { text: None, semibold: false, icons: None });
    ctx.run(RawInput::default(), |ctx| { egui::CentralPanel::default().show(ctx, |ui| ui.label("ok")); });
    assert!(!icons::icons_available(&ctx));
    assert!(icons::icon_text(&ctx, Icon::Home, 16.0).is_none());
}

#[test]
fn corrupt_font_files_are_skipped() {
    let dir = temp_dir_with(&[("SegUIVar.ttf", b"not a font"), ("SegoeIcons.ttf", b"\0\0\0")]);
    let ctx = egui::Context::default();
    let r = fonts::install(&ctx, &dir);
    assert_eq!(r.text, None);
    assert_eq!(r.icons, None);
    ctx.run(RawInput::default(), |ctx| { egui::CentralPanel::default().show(ctx, |ui| ui.label("ok")); });
}

#[test]
fn falls_back_to_the_next_candidate() {
    // copy C:\Windows\Fonts\segoeui.ttf and segmdl2.ttf into a temp dir (skip the test if absent)
    // → text == Some("segoeui.ttf"), icons == Some("segmdl2.ttf")
}

#[test]
fn real_windows_fonts_load_when_present() {
    // skip if C:\Windows\Fonts\SegUIVar.ttf is absent; else text == Some("SegUIVar.ttf"), semibold, icons Some(_)
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --lib kit`
Expected: compile error (`fonts` is missing).

- [ ] **Step 3: Implement `install`, `semibold`, `glyph` and `icon_text` as specified above.**

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test --lib kit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/ui/kit src/ui/mod.rs
git commit -m "feat(kit): load Segoe UI and Fluent icons at runtime with fallbacks"
```

### Task 7: Kit controls

**Files:**
- Create:
  - `src/ui/kit/controls.rs`, for the toggle, segmented choice, pill and
    disclosure;
  - `src/ui/kit/surfaces.rs`, for the card, section, setting row, banner
    and empty state;
  - `src/ui/kit/test_input.rs`: a `#[cfg(test)]` click helper.
- Test: `src/ui/kit/tests.rs`

**Interfaces:**
- Consumes:
  - `current_palette`, `space`, `radius` and `text_size` (Task 4);
  - `icon_text` and `fonts::semibold` (Task 6).
- Produces:

```rust
// controls.rs
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response;                    // animated knob, 40×20, accent when on
pub fn toggle_enabled(ui: &mut Ui, on: &mut bool, disabled_reason: Option<&str>) -> Response; // reason shown on hover, never changes `on`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PillKind { Success, Warning, Accent, Neutral, Danger }
pub fn pill(ui: &mut Ui, text: &str, kind: PillKind) -> Response;
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, value: &mut T, options: &[(T, &str)]) -> Response; // changed() when value changed
pub fn disclosure<R>(ui: &mut Ui, id_salt: &str, title: &str, default_open: bool, body: impl FnOnce(&mut Ui) -> R) -> Option<R>;
// surfaces.rs
pub fn card<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R;
pub fn section<R>(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui) -> R) -> R;  // semibold 15 title + card
pub struct SettingRow<'a> { pub icon: Option<Icon>, pub title: &'a str, pub description: Option<&'a str> }
impl SettingRow<'_> { pub fn show<R>(self, ui: &mut Ui, trailing: impl FnOnce(&mut Ui) -> R) -> R; } // trailing right-aligned
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerKind { Info, Warning, Error }
pub fn banner(ui: &mut Ui, kind: BannerKind, text: &str, action: Option<&str>) -> bool; // true when the action was clicked
pub fn empty_state(ui: &mut Ui, icon: Icon, text: &str, action: Option<&str>) -> bool;
// test_input.rs (cfg(test))
pub fn click_frames(ctx: &egui::Context, at: egui::Pos2, draw: impl FnMut(&egui::Context)); // 3 frames: move, press, release
```

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn clicking_a_toggle_flips_it() { /* draw toggle in CentralPanel, record rect, click_frames at its center → on == true */ }
#[test] fn a_disabled_toggle_never_flips() { /* toggle_enabled with Some("reason") → click → still false */ }
#[test] fn segmented_selects_the_clicked_option() { /* 2 options, click the 2nd → value == B and changed() */ }
#[test] fn banner_reports_its_action_click() { /* click the action button → returns true once */ }
#[test] fn every_control_renders_in_both_themes_without_fonts() {
    /* ctx with no fonts installed, Light then Dark: toggle, pill (each kind), segmented, disclosure,
       card, section, SettingRow with icon Some(Home), banner (each kind), empty_state all render */
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --lib kit`
Expected: compile error.

- [ ] **Step 3: Implement**

The toggle animates with `ui.ctx().animate_bool_responsive`, as in egui's
toggle-switch demo. Every color comes from `current_palette`, with no
literal colors in `controls.rs` or `surfaces.rs`. A component skips its
icon when `icon_text` returns `None`.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test --lib kit`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ui/kit
git commit -m "feat(kit): toggle, pill, segmented, disclosure, cards, rows, banner, empty state"
```

### Task 8: Kit layout and feedback

**Files:**
- Create:
  - `src/ui/kit/nav.rs`, for the sidebar and page header;
  - `src/ui/kit/feedback.rs`, for the action bar, toasts and dialog.
- Test: `src/ui/kit/tests.rs`

**Interfaces:**
- Consumes: Tasks 4, 6 and 7.
- Produces:

```rust
// nav.rs
pub struct NavItem<'a, K> { pub key: K, pub icon: Icon, pub label: &'a str, pub badge: Option<usize>, pub enabled: bool }
pub struct NavGroup<'a, K> { pub title: Option<&'a str>, pub items: Vec<NavItem<'a, K>> }
pub fn nav_sidebar<K: PartialEq + Copy>(ctx: &egui::Context, selected: &mut K, groups: &[NavGroup<'_, K>], footer: &[NavItem<'_, K>]) -> bool; // SidePanel "wintune_nav", width 260; true when selection changed
pub fn page_header(ui: &mut Ui, breadcrumb: &[&str], title: &str, subtitle: Option<&str>, actions: impl FnOnce(&mut Ui));
// feedback.rs
pub enum ActionBarState<'a> { Idle, Busy { label: &'a str, fraction: f32 } }
pub fn action_bar(ctx: &egui::Context, status: &str, state: ActionBarState<'_>, actions: impl FnOnce(&mut Ui)); // TopBottomPanel::bottom("wintune_action_bar"); Busy hides `actions` and shows the progress bar
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind { Success, Info, Error }
#[derive(Default)]
pub struct Toasts { /* queue */ }
impl Toasts {
    pub fn push(&mut self, kind: ToastKind, text: impl Into<String>);
    pub fn show(&mut self, ctx: &egui::Context);   // bottom-right, newest on top, max 3 shown
    pub fn len(&self) -> usize;
}
pub const TOAST_SECONDS: f64 = 4.0;                // Success/Info expire after this (ctx.input time); Error stays until its × is clicked
pub fn dialog(ctx: &egui::Context, id: &str, title: &str, width: f32, body: impl FnOnce(&mut Ui)); // centered modal Area over a dimmed layer
```

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn clicking_a_nav_item_selects_it() { /* 2 groups + footer item; click footer → selected == footer key, returns true */ }
#[test] fn a_disabled_nav_item_cannot_be_selected() { /* "Coming soon" item enabled: false → click → selection unchanged */ }
#[test] fn info_toasts_expire_and_error_toasts_stay() {
    /* push Info + Error at time 0; run a frame at time 0 → len 2;
       frame at time TOAST_SECONDS + 0.1 → len 1 (the error); */
}
#[test] fn a_busy_action_bar_hides_its_actions() {
    /* action_bar with Busy and an actions closure that sets a flag → flag stays false */
}
#[test] fn layout_components_render_without_fonts_in_both_themes() { /* nav_sidebar, page_header, action_bar Idle/Busy, toasts, dialog */ }
```

The time is driven through `RawInput { time: Some(t), .. }`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test --lib kit`
Expected: compile error.

- [ ] **Step 3: Implement as specified in Interfaces.**

- [ ] **Step 4: Run the full suite**

Run: `cargo test --locked && cargo clippy --all-targets --locked -- -D warnings && cargo fmt --check`
Expected: everything PASS.

- [ ] **Step 5: Commit**

```bash
git add src/ui/kit
git commit -m "feat(kit): nav sidebar, page header, action bar, toasts, dialog"
```

### Task 9: Docs and PR

**Files:**
- Modify:
  - `ARCHITECTURE.md`: a section on `i18n`, with the files, the fallback
    and the `Tr` handle, and a section on `ui::kit`, with the tokens and
    the rule "screens never set colors";
  - `CONTRIBUTING.md`: "Adding a language". Copy `strings.resx` to
    `strings.<code>.resx`, translate the values and set `language.name`.
    The completeness and placeholder tests say what is missing.
  - `README.md`: one line on languages (English, Français).

- [ ] **Step 1: Write the three doc changes.**

- [ ] **Step 2: Final verification**

Run: `cargo fmt --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked && cargo build --release --locked`
Expected: all green. The test count is above 161.

- [ ] **Step 3: Check by hand**

Run `target\release\WinTune.exe` and check that:
- the app starts;
- the fonts are Segoe UI;
- the theme follows Windows;
- the old screens still work;
- the tweak titles still show in English.

- [ ] **Step 4: Commit, then open the PR with the owner's permission**

```bash
git add ARCHITECTURE.md CONTRIBUTING.md README.md
git commit -m "docs: i18n and UI kit"
```

Push `feat/ui-foundations` and open the PR "UI redesign 1/6: foundations"
(translations, engine message codes, themes, fonts, kit).
