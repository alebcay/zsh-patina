use syntect::parsing::{
    Scope, SyntaxDefinition, SyntaxSet, SyntaxSetBuilder,
    syntax_definition::{Context, ContextReference, MatchOperation, MatchPattern, Pattern},
};

use crate::{
    config::{
        PrecommandArg, PrecommandConfig, PrecommandMode, PrecommandOption, PrecommandSwitchTo,
    },
    highlighting::{CALLABLE, FUNCTION_CALL, PARAMETER_OPTION, PUNCTUATION_PARAMETER},
};

const IS_END_OF_OPTION: &str = r"[^\w$-]|(?m:$)";
const KEYWORD_BOUNDARY_END: &str = r"(?!=)(?=[^\w_-]|(?m:$))";
const START_OF_LONG_OPTION: &str = r"(?:\s+|^)--(?=[\w$])";
const START_OF_SHORT_OPTION: &str = r"(?:\s+|^)-(?=[\w$])";

struct ExtendedSyntaxSetBuilder {
    syntax_definition: SyntaxDefinition,
    context_ref_precommand_option: ContextReference,
    context_ref_precommand_end_of_options: ContextReference,
    context_ref_single_argument: ContextReference,
    context_ref_single_long_argument: ContextReference,
    context_ref_multiple_arguments: ContextReference,
}

impl ExtendedSyntaxSetBuilder {
    fn new(syntax_definition: SyntaxDefinition) -> Self {
        // work around the fact that ContextReference::Named is private
        let context_ref_precommand_option =
            toml::from_str(r#"Named = "precommand-option""#).unwrap();
        let context_ref_precommand_end_of_options =
            toml::from_str(r#"Named = "precommand-end-of-options""#).unwrap();
        let context_ref_single_argument = toml::from_str(r#"Named = "single-argument""#).unwrap();
        let context_ref_single_long_argument =
            toml::from_str(r#"Named = "single-long-argument""#).unwrap();
        let context_ref_multiple_arguments =
            toml::from_str(r#"Named = "multiple-arguments""#).unwrap();

        Self {
            syntax_definition,
            context_ref_precommand_option,
            context_ref_precommand_end_of_options,
            context_ref_single_argument,
            context_ref_single_long_argument,
            context_ref_multiple_arguments,
        }
    }

    /// Make a list of patterns matching all short options
    fn make_short_options(
        &self,
        options: &[PrecommandOption],
        switching: bool,
        context_ref_arguments: &ContextReference,
    ) -> Vec<Pattern> {
        // collect all matching short options
        let mut exclude = String::new();
        let mut matching = Vec::new();
        for o in options {
            if let Some(short) = &o.short {
                let is_matching = match o.switch_to_mode {
                    Some(mode) => match mode {
                        PrecommandSwitchTo::Arguments => switching,
                    },
                    None => !switching,
                };
                if is_matching
                    || o.arg == PrecommandArg::Required
                    || o.arg == PrecommandArg::Optional
                {
                    exclude.push_str(short);
                }
                if is_matching {
                    matching.push((short, o));
                }
            }
        }

        if matching.is_empty() {
            // nothing to do
            return Vec::new();
        }

        // highlight the groups
        let captures = Some(vec![
            (1, vec![Scope::new(PARAMETER_OPTION).unwrap()]),
            (2, vec![Scope::new(PUNCTUATION_PARAMETER).unwrap()]),
        ]);

        let mut result = Vec::new();

        for (name, matching_option) in matching {
            let needs_switch = match matching_option.switch_to_mode {
                Some(mode) => match mode {
                    PrecommandSwitchTo::Arguments => true,
                },
                None => false,
            };

            match matching_option.arg {
                PrecommandArg::Required => {
                    // create a regex that matches any number of one letter
                    // options except our exceptions, followed by a single
                    // option taking a *required* argument
                    let regex_str_required =
                        format!(r#"(({START_OF_SHORT_OPTION})[\w&&[^{exclude}]]*{name})\s*"#,);

                    let operation = if needs_switch {
                        MatchOperation::Set(vec![
                            self.context_ref_multiple_arguments.clone(),
                            context_ref_arguments.clone(),
                            self.context_ref_single_argument.clone(),
                        ])
                    } else {
                        MatchOperation::Push(vec![self.context_ref_single_argument.clone()])
                    };

                    result.push(Pattern::Match(MatchPattern::new(
                        false,
                        regex_str_required,
                        Vec::new(),
                        captures.clone(),
                        operation,
                        None,
                    )));
                }

                PrecommandArg::Optional => {
                    // Case 1: There is an argument. Create a regex that matches
                    // any number of one letter options except our exceptions,
                    // followed by a single option taking an argument. Use a
                    // look-ahead at the end to determine if this option is
                    // actually followed by an argument or by another option
                    // (starting with '-').
                    let regex_str_arg = format!(
                        r#"(({})[\w&&[^{exclude}]]*{name})(\S+|(\s+(?![\s-])))"#,
                        START_OF_SHORT_OPTION
                    );

                    let operation_arg = if needs_switch {
                        MatchOperation::Set(vec![
                            self.context_ref_multiple_arguments.clone(),
                            context_ref_arguments.clone(),
                            self.context_ref_single_argument.clone(),
                        ])
                    } else {
                        MatchOperation::Push(vec![self.context_ref_single_argument.clone()])
                    };

                    result.push(Pattern::Match(MatchPattern::new(
                        false,
                        regex_str_arg,
                        Vec::new(),
                        captures.clone(),
                        operation_arg,
                        None,
                    )));

                    // Case 2: There is no argument. Create a regex that matches
                    // any number of one letter options except our exceptions,
                    // followed by a single option taking no argument. This is
                    // only necessary if there is a switch. Otherwise, this case
                    // is already covered by the 'precommand-option' context.
                    if needs_switch {
                        let regex_str_no_arg = format!(
                            r#"(({START_OF_SHORT_OPTION})[\w&&[^{exclude}]]*{name})(?={IS_END_OF_OPTION})"#
                        );

                        let operation_no_arg = MatchOperation::Set(vec![
                            self.context_ref_multiple_arguments.clone(),
                            context_ref_arguments.clone(),
                        ]);

                        result.push(Pattern::Match(MatchPattern::new(
                            false,
                            regex_str_no_arg,
                            Vec::new(),
                            captures.clone(),
                            operation_no_arg,
                            None,
                        )));
                    }
                }

                PrecommandArg::None => {
                    // There is no argument. Create a regex that matches any
                    // number of one letter options except our exceptions,
                    // followed by a single option taking no argument and then
                    // optionally followed by more single-letter options. This
                    // is only necessary if there is a switch. Otherwise, this
                    // case is already covered by the 'precommand-option'
                    // context.
                    if needs_switch {
                        let regex_str_no_arg = format!(
                            r#"(({START_OF_SHORT_OPTION})[\w&&[^{exclude}]]*{name}[a-zA-Z]*)(?={IS_END_OF_OPTION})"#,
                        );

                        let operation_no_arg = MatchOperation::Set(vec![
                            self.context_ref_multiple_arguments.clone(),
                            context_ref_arguments.clone(),
                        ]);

                        result.push(Pattern::Match(MatchPattern::new(
                            false,
                            regex_str_no_arg,
                            Vec::new(),
                            captures.clone(),
                            operation_no_arg,
                            None,
                        )));
                    }
                }
            }
        }

        result
    }

    /// Make a list of patterns matching all long options
    fn make_long_options(
        &self,
        options: &[PrecommandOption],
        switching: bool,
        context_ref_arguments: &ContextReference,
    ) -> Vec<Pattern> {
        // collect all matching long options
        let mut matching = Vec::new();
        for o in options {
            if let Some(long) = &o.long {
                let is_matching = match o.switch_to_mode {
                    Some(mode) => match mode {
                        PrecommandSwitchTo::Arguments => switching,
                    },
                    None => !switching,
                };
                if is_matching {
                    matching.push((long, o));
                }
            }
        }

        if matching.is_empty() {
            // nothing to do
            return Vec::new();
        }

        // highlight the groups
        let captures = Some(vec![
            (1, vec![Scope::new(PARAMETER_OPTION).unwrap()]),
            (2, vec![Scope::new(PUNCTUATION_PARAMETER).unwrap()]),
        ]);

        let mut result = Vec::new();

        for (name, matching_option) in matching {
            let needs_switch = match matching_option.switch_to_mode {
                Some(mode) => match mode {
                    PrecommandSwitchTo::Arguments => true,
                },
                None => false,
            };

            match matching_option.arg {
                PrecommandArg::Required => {
                    // create a regex that matches our option followed by a
                    // single argument
                    let regex_str_required = format!(r#"(({START_OF_LONG_OPTION}){name})\s*"#);

                    let operation = if needs_switch {
                        MatchOperation::Set(vec![
                            self.context_ref_multiple_arguments.clone(),
                            context_ref_arguments.clone(),
                            self.context_ref_single_long_argument.clone(),
                        ])
                    } else {
                        MatchOperation::Push(vec![self.context_ref_single_long_argument.clone()])
                    };

                    result.push(Pattern::Match(MatchPattern::new(
                        false,
                        regex_str_required,
                        Vec::new(),
                        captures.clone(),
                        operation,
                        None,
                    )));
                }

                PrecommandArg::Optional => {
                    // Case 1: There is an argument. Create a regex that matches
                    // our option followed by this argument. Use a look-ahead at
                    // the end to determine if this option is actually followed
                    // by an argument or by another option (starting with '-').
                    let regex_str_arg =
                        format!(r#"(({START_OF_LONG_OPTION}){name})((?==)|(\s+(?![\s-])))"#);

                    let operation_arg = if needs_switch {
                        MatchOperation::Set(vec![
                            self.context_ref_multiple_arguments.clone(),
                            context_ref_arguments.clone(),
                            self.context_ref_single_long_argument.clone(),
                        ])
                    } else {
                        MatchOperation::Push(vec![self.context_ref_single_long_argument.clone()])
                    };

                    result.push(Pattern::Match(MatchPattern::new(
                        false,
                        regex_str_arg,
                        Vec::new(),
                        captures.clone(),
                        operation_arg,
                        None,
                    )));

                    // Case 2: There is no argument. Create a regex that matches
                    // our option taking no argument. This is only necessary if
                    // there is a switch. Otherwise, this case is already
                    // covered by the 'precommand-option' context.
                    if needs_switch {
                        let regex_str_no_arg = format!(
                            r#"(({}){name})(?={})"#,
                            START_OF_LONG_OPTION, IS_END_OF_OPTION
                        );

                        let operation_no_arg = MatchOperation::Set(vec![
                            self.context_ref_multiple_arguments.clone(),
                            context_ref_arguments.clone(),
                        ]);

                        result.push(Pattern::Match(MatchPattern::new(
                            false,
                            regex_str_no_arg,
                            Vec::new(),
                            captures.clone(),
                            operation_no_arg,
                            None,
                        )));
                    }
                }

                PrecommandArg::None => {
                    // There is no argument. Create a regex that matches our
                    // option taking no argument. This is only necessary if
                    // there is a switch. Otherwise, this case is already
                    // covered by the 'precommand-option' context.
                    if needs_switch {
                        let regex_str_no_arg =
                            format!("(({START_OF_LONG_OPTION}){name})(?={IS_END_OF_OPTION})");

                        let operation_no_arg = MatchOperation::Set(vec![
                            self.context_ref_multiple_arguments.clone(),
                            context_ref_arguments.clone(),
                        ]);

                        result.push(Pattern::Match(MatchPattern::new(
                            false,
                            regex_str_no_arg,
                            Vec::new(),
                            captures.clone(),
                            operation_no_arg,
                            None,
                        )));
                    }
                }
            }
        }

        result
    }

    /// Add a dynamic syntax definition for a precommand with a given unique ID
    fn add_precommand(&mut self, config: &PrecommandConfig, id: usize) {
        let context_ref_base_name = format!("precommand-{id}-base");
        let context_ref_default_name = format!("precommand-{id}-default");
        let context_ref_arguments_name = format!("precommand-{id}-arguments");

        // work around the fact that ContextReference::Named is private
        let context_ref_base: ContextReference =
            toml::from_str(&format!(r#"Named = "{context_ref_base_name}""#)).unwrap();
        let context_ref_default: ContextReference =
            toml::from_str(&format!(r#"Named = "{context_ref_default_name}""#)).unwrap();
        let context_ref_arguments: ContextReference =
            toml::from_str(&format!(r#"Named = "{context_ref_arguments_name}""#)).unwrap();

        let mut base_patterns = Vec::new();

        // add options but don't switch context
        for p in self.make_short_options(&config.options, false, &context_ref_arguments) {
            base_patterns.push(p);
        }
        for p in self.make_long_options(&config.options, false, &context_ref_arguments) {
            base_patterns.push(p);
        }

        let mut switch_to_arguments_patterns = Vec::new();

        // add options switching the context
        for p in self.make_short_options(&config.options, true, &context_ref_arguments) {
            switch_to_arguments_patterns.push(p);
        }
        for p in self.make_long_options(&config.options, true, &context_ref_arguments) {
            switch_to_arguments_patterns.push(p);
        }

        let mut base_context = Context::new(true);
        base_context.patterns.extend(base_patterns);

        let mut default_context = Context::new(true);
        default_context
            .patterns
            .extend(switch_to_arguments_patterns);

        default_context
            .patterns
            .push(Pattern::Include(context_ref_base.clone()));

        // if none of the above patterns for short and long options have
        // matched, fall back to matching any short or long option without an
        // argument
        default_context
            .patterns
            .push(Pattern::Include(self.context_ref_precommand_option.clone()));

        // finally, match the end of options indicator (`--`)
        default_context.patterns.push(Pattern::Include(
            self.context_ref_precommand_end_of_options.clone(),
        ));

        let mut arguments_and_commands_context = Context::new(true);
        arguments_and_commands_context
            .patterns
            .push(Pattern::Include(context_ref_base.clone()));
        arguments_and_commands_context
            .patterns
            .push(Pattern::Include(self.context_ref_precommand_option.clone()));
        arguments_and_commands_context
            .patterns
            .push(Pattern::Include(
                self.context_ref_precommand_end_of_options.clone(),
            ));

        self.syntax_definition
            .contexts
            .insert(context_ref_base_name, base_context);
        self.syntax_definition
            .contexts
            .insert(context_ref_default_name, default_context);
        self.syntax_definition
            .contexts
            .insert(context_ref_arguments_name, arguments_and_commands_context);

        // add main pattern to `control` context
        let pattern = Pattern::Match(MatchPattern::new(
            false,
            format!(
                r#"\b{}{KEYWORD_BOUNDARY_END}"#,
                regex_syntax::escape(&config.name),
            ),
            vec![
                Scope::new(FUNCTION_CALL).unwrap(),
                Scope::new(CALLABLE).unwrap(),
            ],
            None,
            match config.mode {
                PrecommandMode::Default => MatchOperation::Set(vec![context_ref_default]),
                PrecommandMode::Arguments => MatchOperation::Set(vec![
                    self.context_ref_multiple_arguments.clone(),
                    context_ref_arguments,
                ]),
            },
            None,
        ));
        let control = self.syntax_definition.contexts.get_mut("control").unwrap();
        control.patterns.push(pattern);
    }

    fn build(self) -> SyntaxSet {
        let mut syntax_set_builder = SyntaxSetBuilder::new();
        syntax_set_builder.add(self.syntax_definition);
        syntax_set_builder.build()
    }
}

/// Load the syntax set, including dynamically generated contexts for
/// precommands (if configured)
pub fn load_syntax_set(precommands: &[PrecommandConfig]) -> SyntaxSet {
    if precommands.is_empty() {
        // fast path - load original syntax dump
        syntect::dumps::from_uncompressed_data(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/syntax_set.packdump"
        )))
        .expect("Unable to load shell syntax")
    } else {
        // the user has configured precommands - we need to build our own syntax
        // based on the original syntax definition
        let syntax_definition: SyntaxDefinition = syntect::dumps::from_binary(include_bytes!(
            concat!(env!("OUT_DIR"), "/syntax_definition.packdump")
        ));

        let mut essb = ExtendedSyntaxSetBuilder::new(syntax_definition);

        for (i, config) in precommands.iter().enumerate() {
            essb.add_precommand(config, i);
        }

        essb.build()
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use insta::assert_snapshot;

    use crate::{
        config::{
            PrecommandArg, PrecommandConfig, PrecommandMode, PrecommandOption, PrecommandSwitchTo,
        },
        highlighting::highlighter::tests::{TestCfg, test_cfg_with, test_config},
    };

    fn test_cfg_with_precommands(precommands: Vec<PrecommandConfig>) -> Result<TestCfg> {
        let mut config = test_config();
        config.precommands = precommands;
        test_cfg_with(config)
    }

    /// Custom `nice2` precommand: tests `arg: Required` with combined
    /// `short`+`long` option, `mode: Default`
    #[test]
    fn nice2() -> Result<()> {
        let cfg = test_cfg_with_precommands(vec![PrecommandConfig {
            name: "nice2".to_string(),
            mode: PrecommandMode::Default,
            options: vec![PrecommandOption {
                short: Some("n".to_string()),
                long: Some("adjustment".to_string()),
                ..Default::default()
            }],
        }])?;

        assert_snapshot!("nice2__simple", cfg.highlight("nice2 ls")?);
        assert_snapshot!("nice2__n", cfg.highlight("nice2 -n 5 date")?);
        assert_snapshot!("nice2__n_no_space", cfg.highlight("nice2 -n5 date")?);
        assert_snapshot!(
            "nice2__adjustment",
            cfg.highlight("nice2 --adjustment 5 date")?
        );
        assert_snapshot!(
            "nice2__adjustment_equals",
            cfg.highlight("nice2 --adjustment=5 date")?
        );

        Ok(())
    }

    /// Custom `nohup2` precommand: tests `mode: Default` with an empty options
    /// list. The precommand takes no options and simply passes through to a
    /// command — analogous to `nohup`.
    #[test]
    fn nohup2() -> Result<()> {
        let cfg = test_cfg_with_precommands(vec![PrecommandConfig {
            name: "nohup2".to_string(),
            mode: PrecommandMode::Default,
            options: vec![],
        }])?;

        assert_snapshot!("nohup2__simple", cfg.highlight("nohup2 ls")?);
        assert_snapshot!("nohup2__end_of_options", cfg.highlight("nohup2 -- ls")?);

        Ok(())
    }

    /// Custom `doas2` precommand: tests `arg: None` (plain flag) and `arg:
    /// Required` on short-only options.
    #[test]
    fn doas2() -> Result<()> {
        let cfg = test_cfg_with_precommands(vec![PrecommandConfig {
            name: "doas2".to_string(),
            mode: PrecommandMode::Default,
            options: vec![
                PrecommandOption {
                    short: Some("n".to_string()),
                    arg: PrecommandArg::None,
                    ..Default::default()
                },
                PrecommandOption {
                    short: Some("u".to_string()),
                    ..Default::default()
                },
                PrecommandOption {
                    short: Some("C".to_string()),
                    ..Default::default()
                },
            ],
        }])?;

        assert_snapshot!(
            "doas2__n_u_c",
            cfg.highlight("doas2 -n -u root -C doas.conf ls")?
        );

        cfg.touch_file("doas.conf")?;

        assert_snapshot!(
            "doas2__n_u_c_conf_file_exists",
            cfg.highlight("doas2 -n -uroot -Cdoas.conf -- ls")?
        );

        Ok(())
    }

    /// Custom `env2` precommand: tests combined `short`+`long` on the same
    /// option (e.g. `-u`/`--unset` and `-C`/`--chdir` are the same option).
    #[test]
    fn env2() -> Result<()> {
        let cfg = test_cfg_with_precommands(vec![PrecommandConfig {
            name: "env2".to_string(),
            mode: PrecommandMode::Default,
            options: vec![
                PrecommandOption {
                    short: Some("i".to_string()),
                    arg: PrecommandArg::None,
                    ..Default::default()
                },
                PrecommandOption {
                    short: Some("u".to_string()),
                    long: Some("unset".to_string()),
                    ..Default::default()
                },
                PrecommandOption {
                    short: Some("C".to_string()),
                    long: Some("chdir".to_string()),
                    ..Default::default()
                },
            ],
        }])?;

        cfg.create_dir("mydir")?;

        assert_snapshot!("env2__i", cfg.highlight("env2 -i ls")?);
        assert_snapshot!("env2__u", cfg.highlight("env2 -u _ env2")?);
        assert_snapshot!("env2__unset", cfg.highlight("env2 --unset _ env2")?);
        assert_snapshot!("env2__unset_equals", cfg.highlight("env2 --unset=_ env2")?);
        assert_snapshot!("env2__c", cfg.highlight("env2 -C mydir env2")?);
        assert_snapshot!("env2__chdir", cfg.highlight("env2 --chdir mydir env2")?);
        assert_snapshot!("env2__iu", cfg.highlight("env2 -iu _ env2")?);
        assert_snapshot!(
            "env2__i_unset_equals",
            cfg.highlight("env2 -i --unset=_ env2")?
        );

        Ok(())
    }

    /// Custom `sudo2` precommand: tests `arg: Optional`, `switch_to_mode:
    /// Some(Arguments)`, combined `short`+`long` options, and a long-only
    /// option.
    #[test]
    fn sudo2() -> Result<()> {
        let cfg = test_cfg_with_precommands(vec![PrecommandConfig {
            name: "sudo2".to_string(),
            mode: PrecommandMode::Default,
            options: vec![
                PrecommandOption {
                    short: Some("n".to_string()),
                    arg: PrecommandArg::None,
                    ..Default::default()
                },
                PrecommandOption {
                    short: Some("u".to_string()),
                    long: Some("user".to_string()),
                    ..Default::default()
                },
                PrecommandOption {
                    short: None,
                    long: Some("chdir".to_string()),
                    ..Default::default()
                },
                PrecommandOption {
                    short: Some("h".to_string()),
                    long: Some("host".to_string()),
                    arg: PrecommandArg::Optional,
                    ..Default::default()
                },
                PrecommandOption {
                    short: Some("e".to_string()),
                    long: Some("edit".to_string()),
                    arg: PrecommandArg::None,
                    switch_to_mode: Some(PrecommandSwitchTo::Arguments),
                },
            ],
        }])?;

        cfg.touch_file("file1")?;
        cfg.touch_file("file2")?;
        cfg.create_dir("mydir")?;

        assert_snapshot!("sudo2__simple", cfg.highlight("sudo2 ls")?);
        assert_snapshot!("sudo2__n", cfg.highlight("sudo2 -n ls")?);
        assert_snapshot!(
            "sudo2__n_u_end_of_options",
            cfg.highlight("sudo2 -n -u root -- ls")?
        );
        assert_snapshot!("sudo2__user_equals", cfg.highlight("sudo2 --user=root ls")?);
        assert_snapshot!("sudo2__chdir", cfg.highlight("sudo2 --chdir mydir ls")?);
        // -h / --host with arg: Optional — no argument (followed by &&)
        assert_snapshot!("sudo2__h_no_arg", cfg.highlight("sudo2 -h && ls")?);
        assert_snapshot!("sudo2__host_no_arg", cfg.highlight("sudo2 --host && ls")?);
        // -h / --host with arg: Optional — with argument
        assert_snapshot!("sudo2__h_hostname", cfg.highlight("sudo2 -h localhost")?);
        assert_snapshot!(
            "sudo2__host_hostname",
            cfg.highlight("sudo2 --host localhost")?
        );
        // -e / --edit: switch_to_mode = Arguments
        assert_snapshot!("sudo2__e", cfg.highlight("sudo2 -e file1 file2")?);
        assert_snapshot!("sudo2__edit", cfg.highlight("sudo2 --edit file1 file2")?);

        Ok(())
    }

    /// Custom `sudoedit2` precommand: tests `mode: Arguments` with options.
    /// After the precommand name, tokens are treated as plain file arguments
    /// (no callable lookup) — analogous to `sudoedit` in the built-in syntax.
    #[test]
    fn sudoedit2() -> Result<()> {
        let cfg = test_cfg_with_precommands(vec![PrecommandConfig {
            name: "sudoedit2".to_string(),
            mode: PrecommandMode::Arguments,
            options: vec![
                PrecommandOption {
                    short: Some("u".to_string()),
                    long: Some("user".to_string()),
                    ..Default::default()
                },
                PrecommandOption {
                    short: Some("g".to_string()),
                    long: Some("group".to_string()),
                    ..Default::default()
                },
            ],
        }])?;

        cfg.touch_file("file1")?;
        cfg.touch_file("file2")?;

        assert_snapshot!("sudoedit2__simple", cfg.highlight("sudoedit2 file1 file2")?);
        assert_snapshot!("sudoedit2__u", cfg.highlight("sudoedit2 -u root file1")?);
        assert_snapshot!(
            "sudoedit2__user_equals",
            cfg.highlight("sudoedit2 --user=root file1")?
        );
        assert_snapshot!(
            "sudoedit2__u_g",
            cfg.highlight("sudoedit2 -u root -g wheel file1 file2")?
        );

        Ok(())
    }

    #[test]
    fn zap2() -> Result<()> {
        let cfg = test_cfg_with_precommands(vec![PrecommandConfig {
            name: "zap2".to_string(),
            mode: PrecommandMode::Default,
            options: vec![
                PrecommandOption {
                    short: Some("v".to_string()),
                    long: Some("verbose".to_string()),
                    arg: PrecommandArg::None,
                    ..Default::default()
                },
                PrecommandOption {
                    short: Some("f".to_string()),
                    long: Some("format".to_string()),
                    ..Default::default()
                },
                PrecommandOption {
                    short: Some("x".to_string()),
                    long: Some("exec".to_string()),
                    switch_to_mode: Some(PrecommandSwitchTo::Arguments),
                    ..Default::default()
                },
                PrecommandOption {
                    short: Some("p".to_string()),
                    long: Some("pipe".to_string()),
                    arg: PrecommandArg::Optional,
                    switch_to_mode: Some(PrecommandSwitchTo::Arguments),
                },
            ],
        }])?;

        // normal options — stays in Default mode
        assert_snapshot!("zap2__v", cfg.highlight("zap2 -v ls")?);
        assert_snapshot!("zap2__verbose", cfg.highlight("zap2 --verbose ls")?);
        assert_snapshot!("zap2__f", cfg.highlight("zap2 -f json ls")?);
        assert_snapshot!("zap2__format", cfg.highlight("zap2 --format json ls")?);
        assert_snapshot!(
            "zap2__format_equals",
            cfg.highlight("zap2 --format=json ls")?
        );

        // Required + switch_to_mode
        assert_snapshot!("zap2__x", cfg.highlight("zap2 -x cmd arg1")?);
        assert_snapshot!("zap2__exec", cfg.highlight("zap2 --exec cmd arg1")?);
        assert_snapshot!("zap2__exec_equals", cfg.highlight("zap2 --exec=cmd arg1")?);

        cfg.create_dir("pipedir")?;

        // Optional + switch_to_mode, with argument
        assert_snapshot!("zap2__p_with_arg", cfg.highlight("zap2 -p pipedir arg1")?);
        assert_snapshot!(
            "zap2__pipe_with_arg",
            cfg.highlight("zap2 --pipe pipedir arg1")?
        );

        // Optional + switch_to_mode, without argument
        assert_snapshot!("zap2__p_no_arg", cfg.highlight("zap2 -p arg1")?);
        assert_snapshot!("zap2__pipe_no_arg", cfg.highlight("zap2 --pipe arg1")?);

        // combinations: normal option before switching option
        assert_snapshot!("zap2__v_x", cfg.highlight("zap2 -v -x cmd arg1")?);
        assert_snapshot!("zap2__vx", cfg.highlight("zap2 -vx cmd arg1")?);
        assert_snapshot!("zap2__xv", cfg.highlight("zap2 -x cmd -v arg1")?);
        assert_snapshot!("zap2__f_x", cfg.highlight("zap2 -f json -x cmd arg1")?);
        assert_snapshot!("zap2__vf_x", cfg.highlight("zap2 -vf json -x cmd arg1")?);
        assert_snapshot!(
            "zap2__v_p_with_arg",
            cfg.highlight("zap2 -v -p pipedir arg1")?
        );
        assert_snapshot!("zap2__v_p_no_arg", cfg.highlight("zap2 -v -p arg1")?);
        assert_snapshot!("zap2__vp_no_arg", cfg.highlight("zap2 -vp arg1")?);
        assert_snapshot!("zap2__vp_arg", cfg.highlight("zap2 -vp pipedir arg1")?);

        Ok(())
    }
}
