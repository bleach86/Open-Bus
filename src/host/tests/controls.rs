//! What's bound: the defaults, binding over them, and keeping them.

#[cfg(test)]
mod tests {
    use crate::host::controls::{ACTIONS, Action, Controls, said};

    #[test]
    fn the_defaults_are_the_controls_as_they_were() {
        let controls = Controls::default();
        let has = |action, source: &str| controls.of(0, action).contains(&source);
        assert!(has(Action::A, "key:X") && has(Action::A, "pad:a"));
        assert!(
            has(Action::B, "key:Z") && has(Action::B, "pad:x"),
            "West, as the pads have it"
        );
        assert!(has(Action::Select, "key:Right Shift") && has(Action::Select, "key:Tab"));
        assert!(
            has(Action::Up, "pad:dpup") && has(Action::Up, "pad:lefty-"),
            "the D-pad and the stick"
        );
        assert!(has(Action::Library, "key:F5") && has(Action::Library, "pad:guide"));
    }

    #[test]
    fn the_second_player_has_a_gamepad_and_no_keys() {
        let controls = Controls::default();
        assert_eq!(controls.of(1, Action::A), ["pad:a"]);
        assert!(
            controls.of(1, Action::Library).is_empty(),
            "there's only one way back"
        );
    }

    #[test]
    fn a_key_replaces_the_key_and_leaves_the_button() {
        let mut controls = Controls::default();
        controls.bind(0, Action::A, "key:C");
        let bound = controls.of(0, Action::A);
        assert!(bound.contains(&"pad:a") && bound.contains(&"key:C") && !bound.contains(&"key:X"));
    }

    #[test]
    fn a_button_replaces_the_button_and_leaves_the_key_and_the_stick() {
        let mut controls = Controls::default();
        controls.bind(0, Action::Up, "pad:y");
        let bound = controls.of(0, Action::Up);
        assert!(
            bound.contains(&"pad:y") && bound.contains(&"key:Up") && bound.contains(&"pad:lefty-")
        );
        assert!(!bound.contains(&"pad:dpup"));
    }

    #[test]
    fn a_thing_bound_again_leaves_what_it_was_for() {
        let mut controls = Controls::default();
        controls.bind(0, Action::A, "key:Z"); // B's
        assert!(controls.of(0, Action::A).contains(&"key:Z"));
        assert!(!controls.of(0, Action::B).contains(&"key:Z"));
    }

    #[test]
    fn a_player_keeps_their_own_when_the_other_binds() {
        let mut controls = Controls::default();
        controls.bind(1, Action::A, "pad:b");
        assert!(controls.of(0, Action::A).contains(&"pad:a"));
    }

    #[test]
    fn it_holds_what_any_binding_says() {
        let controls = Controls::default();
        assert!(controls.holds(0, Action::A, |source| source == "key:X"));
        assert!(controls.holds(0, Action::A, |source| source == "pad:a"));
        assert!(!controls.holds(0, Action::A, |source| source == "pad:b"));
    }

    #[test]
    fn what_is_kept_is_what_comes_back() {
        let mut controls = Controls::default();
        controls.bind(0, Action::Select, "key:Left Shift");
        controls.bind(1, Action::Start, "pad:lefttrigger");
        let back = Controls::parse(&controls.text());
        assert_eq!(back, controls);
        assert!(
            back.of(0, Action::Select).contains(&"key:Left Shift"),
            "a name with a space"
        );
    }

    #[test]
    fn a_file_with_nothing_in_it_is_the_defaults() {
        assert_eq!(Controls::parse(""), Controls::default());
        assert_eq!(
            Controls::parse("nonsense\n9 a key:X\n1 fly key:X\n"),
            Controls::default()
        );
    }

    #[test]
    fn a_bad_line_is_left_out_and_the_rest_kept() {
        let controls = Controls::parse("1 a key:Q\nwhat\n");
        assert_eq!(controls.of(0, Action::A), ["key:Q"]);
    }

    #[test]
    fn a_player_can_be_put_back() {
        let mut controls = Controls::default();
        controls.bind(0, Action::A, "key:C");
        controls.bind(1, Action::A, "pad:b");
        controls.reset(0);
        assert!(controls.of(0, Action::A).contains(&"key:X"));
        assert_eq!(controls.of(1, Action::A), ["pad:b"], "the other's left");
    }

    #[test]
    fn every_action_has_something_for_the_first_player() {
        let controls = Controls::default();
        assert!(
            ACTIONS
                .iter()
                .all(|&action| !controls.of(0, action).is_empty())
        );
    }

    #[test]
    fn fast_forward_is_the_first_players_backquote_and_right_trigger() {
        let controls = Controls::default();
        assert_eq!(
            controls.of(0, Action::FastForward),
            ["key:`", "pad:righttrigger"]
        );
        assert!(controls.of(1, Action::FastForward).is_empty());
    }

    #[test]
    fn a_file_from_before_an_action_has_its_defaults_not_bound_elsewhere() {
        // Before fast-forward, the second player's A on B, and their A
        // bound to nothing.
        let mut old = Controls::default();
        old.bind(1, Action::B, "pad:a");
        let text: String = old
            .text()
            .lines()
            .filter(|line| !line.contains(" fast "))
            .map(|line| format!("{line}\n"))
            .collect();
        let controls = Controls::parse(&text);
        assert_eq!(
            controls.of(0, Action::FastForward),
            ["key:`", "pad:righttrigger"]
        );
        assert!(
            controls.of(1, Action::A).is_empty(),
            "A's default given back, though it's B's"
        );
    }

    #[test]
    fn turbo_is_each_players_s_and_a_keys_and_the_east_and_north_buttons() {
        let controls = Controls::default();
        assert_eq!(controls.of(0, Action::TurboA), ["pad:b", "key:S"]);
        assert_eq!(controls.of(0, Action::TurboB), ["pad:y", "key:A"]);
        assert_eq!(
            controls.of(1, Action::TurboA),
            ["pad:b"],
            "the second's gamepad"
        );
    }

    #[test]
    fn bindings_are_said_for_people() {
        assert_eq!(said("key:Right Shift"), "RIGHT SHIFT");
        assert_eq!(said("pad:a"), "PAD A", "not to be taken for a key");
        assert_eq!(said("pad:dpup"), "PAD UP");
    }
}
