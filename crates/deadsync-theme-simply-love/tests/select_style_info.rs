use deadlib_present::{
    actors::Actor,
    space::{self, Metrics},
};
use deadsync_profile::PlayStyle;
use deadsync_theme_simply_love::{i18n, screens::select_style, views::SelectFlowRuntimeView};

#[test]
fn explanation_bubbles_use_every_locale_and_fit_both_screen_ratios() {
    for locale in [
        "en", "fr", "de", "es", "it", "ja", "pl", "pt-br", "ru", "sv", "pseudo",
    ] {
        let bundle = deadsync_assets::language::load_for_tests(locale);
        let language = if locale == "en" {
            &bundle.fallback
        } else {
            &bundle.active
        };
        let expected: Vec<_> = ["DoubleInfo", "CoupleInfo", "RoutineInfo"]
            .into_iter()
            .map(|key| {
                language
                    .get("SelectStyle")
                    .and_then(|section| section.get(key))
                    .unwrap_or_else(|| panic!("missing {locale} SelectStyle/{key}"))
                    .clone()
            })
            .collect();
        i18n::set_locale(bundle);
        for width in [640.0, 480.0 * 16.0 / 9.0] {
            space::set_current_metrics(Metrics::centered(width, 480.0));
            for (index, style) in [PlayStyle::Double, PlayStyle::Couple, PlayStyle::Routine]
                .into_iter()
                .enumerate()
            {
                let mut state = select_style::init(SelectFlowRuntimeView {
                    play_style: style,
                    ..Default::default()
                });
                select_style::update(&mut state, 0.2);
                let actors = select_style::get_actors(&state);
                assert!(actors.iter().any(|actor| matches!(actor,
                    Actor::Text { content, z: 13, .. } if content.as_str() == expected[index].as_ref()
                )), "{locale} {style:?}");
                let mut bodies = 0;
                for actor in &actors {
                    if let Actor::Mesh {
                        offset,
                        size,
                        z: 12,
                        ..
                    } = actor
                    {
                        let deadlib_present::actors::SizeSpec::Px(w) = size[0] else {
                            panic!("pixel bubble width")
                        };
                        assert!(offset[0] - w * 0.5 >= 0.0, "{locale} {style:?} left edge");
                        assert!(
                            offset[0] + w * 0.5 <= width,
                            "{locale} {style:?} right edge"
                        );
                        bodies += 1;
                    }
                }
                assert_eq!(bodies, 1);
            }
        }
    }
}
