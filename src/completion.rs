//! Static shell completion derived from the built CLI grammar. Completion never
//! calls WEDL, reads a world or delegates to filesystem completion.
use clap::{Arg, Command, ValueEnum};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use wedl::Result;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Shell {
    Bash,
    Powershell,
}

#[derive(Clone, Serialize)]
struct Values {
    min: usize,
    // -1 denotes a variadic argument; the shell only counts supplied words.
    max: i64,
    hyphen: bool,
    negative: bool,
    equals: bool,
    choices: Vec<String>,
}

impl Values {
    fn from(arg: &Arg) -> Self {
        let range = arg
            .get_num_args()
            .expect("built argument has a value range");
        Self {
            min: range.min_values(),
            max: if range.max_values() == usize::MAX {
                -1
            } else {
                range.max_values() as i64
            },
            hyphen: arg.is_allow_hyphen_values_set(),
            negative: arg.is_allow_negative_numbers_set(),
            equals: arg.is_require_equals_set(),
            choices: if arg.is_hide_set() || arg.is_hide_possible_values_set() {
                vec![]
            } else {
                arg.get_possible_values()
                    .into_iter()
                    .filter(|value| !value.is_hide_set())
                    .map(|value| value.get_name().to_owned())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect()
            },
        }
    }
}

#[derive(Serialize)]
struct OptionWord {
    word: String,
    visible: bool,
    #[serde(flatten)]
    values: Values,
}

#[derive(Serialize)]
struct Child {
    word: String,
    node: usize,
    visible: bool,
}

#[derive(Default, Serialize)]
struct Node {
    children: Vec<Child>,
    options: Vec<OptionWord>,
    positions: Vec<Values>,
}

fn collect(command: &Command, nodes: &mut Vec<Node>) -> usize {
    let index = nodes.len();
    nodes.push(Node::default());
    let mut node = Node::default();
    let mut positions = BTreeMap::new();
    let mut options = BTreeMap::new();
    for arg in command.get_arguments() {
        if let Some(position) = arg.get_index() {
            positions.insert(position, Values::from(arg));
            continue;
        }
        let mut all = BTreeSet::new();
        let mut visible = BTreeSet::new();
        if let Some(name) = arg.get_long() {
            all.insert(format!("--{name}"));
            visible.insert(format!("--{name}"));
        }
        for name in arg.get_all_aliases().unwrap_or_default() {
            all.insert(format!("--{name}"));
        }
        for name in arg.get_visible_aliases().unwrap_or_default() {
            visible.insert(format!("--{name}"));
        }
        if let Some(name) = arg.get_short() {
            all.insert(format!("-{name}"));
            visible.insert(format!("-{name}"));
        }
        for name in arg.get_all_short_aliases().unwrap_or_default() {
            all.insert(format!("-{name}"));
        }
        for name in arg.get_visible_short_aliases().unwrap_or_default() {
            visible.insert(format!("-{name}"));
        }
        for word in all {
            options.insert(
                word.clone(),
                OptionWord {
                    visible: !arg.is_hide_set() && visible.contains(&word),
                    word,
                    values: Values::from(arg),
                },
            );
        }
    }
    node.options = options.into_values().collect();
    node.positions = positions.into_values().collect();
    let mut children = BTreeMap::new();
    for child in command.get_subcommands() {
        let child_index = collect(child, nodes);
        let visible = child
            .get_name_and_visible_aliases()
            .into_iter()
            .collect::<BTreeSet<_>>();
        for word in std::iter::once(child.get_name()).chain(child.get_all_aliases()) {
            children.insert(
                word.to_owned(),
                Child {
                    word: word.to_owned(),
                    node: child_index,
                    visible: !child.is_hide_set() && visible.contains(word),
                },
            );
        }
    }
    node.children = children.into_values().collect();
    nodes[index] = node;
    index
}

fn bash_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn bash_words<'a>(values: impl IntoIterator<Item = &'a str>) -> String {
    values
        .into_iter()
        .map(bash_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

fn bash_values(value: &Values) -> String {
    format!(
        "_wr_min={} _wr_max={} _wr_hyphen={} _wr_negative={} _wr_equals={}; _wr_choices=({})",
        value.min,
        value.max,
        u8::from(value.hyphen),
        u8::from(value.negative),
        u8::from(value.equals),
        bash_words(value.choices.iter().map(String::as_str)),
    )
}

fn fill(template: &str, replacements: &[(&str, &str)]) -> String {
    // Locate placeholders in the template only: parser literals that happen to
    // contain a placeholder spelling must remain inert, exact data.
    let mut edits = replacements
        .iter()
        .map(|(name, value)| {
            (
                template.find(name).expect("known template placeholder"),
                name.len(),
                *value,
            )
        })
        .collect::<Vec<_>>();
    edits.sort_by_key(|(offset, _, _)| std::cmp::Reverse(*offset));
    let mut result = template.to_owned();
    for (offset, length, value) in edits {
        result.replace_range(offset..offset + length, value);
    }
    result
}

fn bash(command: &str, nodes: &[Node]) -> String {
    let mut option_cases = String::new();
    let mut position_cases = String::new();
    let mut child_cases = String::new();
    let mut candidate_cases = String::new();
    for (index, node) in nodes.iter().enumerate() {
        for option in &node.options {
            option_cases.push_str(&format!(
                "        {}) {} ;;\n",
                bash_quote(&format!("{index}:{}", option.word)),
                bash_values(&option.values),
            ));
        }
        for (position, value) in node.positions.iter().enumerate() {
            position_cases.push_str(&format!(
                "        '{index}:{position}') {} ;;\n",
                bash_values(value),
            ));
        }
        for child in &node.children {
            child_cases.push_str(&format!(
                "        {}) _wr_child={} ;;\n",
                bash_quote(&format!("{index}:{}", child.word)),
                child.node,
            ));
        }
        candidate_cases.push_str(&format!(
            "        {index}) _wr_options=({}); _wr_commands=({}) ;;\n",
            bash_words(
                node.options
                    .iter()
                    .filter(|value| value.visible)
                    .map(|value| value.word.as_str())
            ),
            bash_words(
                node.children
                    .iter()
                    .filter(|value| value.visible)
                    .map(|value| value.word.as_str())
            ),
        ));
    }
    fill(
        BASH,
        &[
            ("__OPTIONS__", &option_cases),
            ("__POSITIONS__", &position_cases),
            ("__CHILDREN__", &child_cases),
            ("__CANDIDATES__", &candidate_cases),
            ("__COMMAND__", &bash_quote(command)),
        ],
    )
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Render only immutable parser metadata. The caller emits this before source
/// loading; no command help text, defaults or authored data enter the scripts.
pub fn render(mut command: Command, shell: Shell) -> Result<String> {
    command.build();
    let mut nodes = Vec::new();
    collect(&command, &mut nodes);
    Ok(match shell {
        Shell::Bash => bash(command.get_name(), &nodes),
        Shell::Powershell => fill(
            POWERSHELL,
            &[
                (
                    "__NODES__",
                    &powershell_quote(&serde_json::to_string(&nodes)?),
                ),
                ("__COMMAND__", &powershell_quote(command.get_name())),
                (
                    "__COMMAND_EXE__",
                    &powershell_quote(&format!("{}.exe", command.get_name())),
                ),
            ],
        ),
    })
}

const BASH: &str = r#"# Bash completion for WEDL, generated from its actual Clap command tree.
# Save this output to a file and source it. Tab never runs WEDL or lists files.
_wedl_rs_option() {
    case "$1:$2" in
__OPTIONS__        *) return 1 ;;
    esac
}
_wedl_rs_position() {
    case "$1:$2" in
__POSITIONS__        *) return 1 ;;
    esac
}
_wedl_rs_child() {
    case "$1:$2" in
__CHILDREN__        *) return 1 ;;
    esac
}
_wedl_rs_candidates() {
    _wr_options=() _wr_commands=()
    case "$1" in
__CANDIDATES__    esac
}
_wedl_rs_literal() {
    # Decode literal shell quoting only. Never evaluate expansions or commands.
    local _wr_raw=$1 _wr_partial=$2 _wr_k _wr_char _wr_next
    _wr_literal= _wr_quote=
    for ((_wr_k=0; _wr_k<${#_wr_raw}; _wr_k++)); do
        _wr_char=${_wr_raw:_wr_k:1}
        if [[ $_wr_quote == "'" ]]; then
            if [[ $_wr_char == "'" ]]; then _wr_quote=; else _wr_literal+=$_wr_char; fi
        elif [[ $_wr_quote == '"' ]]; then
            case $_wr_char in
                '"') _wr_quote= ;;
                '$'|'`') return 1 ;;
                '\')
                    _wr_k=$((_wr_k+1)); ((_wr_k < ${#_wr_raw})) || return 1
                    _wr_next=${_wr_raw:_wr_k:1}
                    case $_wr_next in
                        '$'|'`'|'"'|'\') _wr_literal+=$_wr_next ;;
                        $'\n') ;;
                        *) _wr_literal+=\\$_wr_next ;;
                    esac ;;
                *) _wr_literal+=$_wr_char ;;
            esac
        else
            case $_wr_char in
                "'"|'"') _wr_quote=$_wr_char ;;
                '\')
                    _wr_k=$((_wr_k+1)); ((_wr_k < ${#_wr_raw})) || return 1
                    _wr_next=${_wr_raw:_wr_k:1}
                    [[ $_wr_next == $'\n' ]] || _wr_literal+=$_wr_next ;;
                '$'|'`'|'('|')'|'<'|'>'|'|'|'&'|';'|'*'|'?'|'['|'{'|'}') return 1 ;;
                *) _wr_literal+=$_wr_char ;;
            esac
        fi
    done
    [[ -z $_wr_quote || $_wr_partial == 1 ]]
}
_wedl_rs_completion() {
    # Byte offsets agree with COMP_POINT even when literal arguments use UTF-8.
    local LC_ALL=C
    local _wr_node=0 _wr_position=0 _wr_used=0 _wr_seen=0 _wr_enabled=1
    local _wr_pending=0 _wr_pnode=0 _wr_pword= _wr_count=0
    local _wr_pmin=0 _wr_pmax=0 _wr_phyphen=0 _wr_pnegative=0
    local _wr_min=0 _wr_max=0 _wr_hyphen=0 _wr_negative=0 _wr_equals=0 _wr_child=0
    local _wr_i _wr_j _wr_word _wr_name _wr_rest _wr_attached _wr_prefix _wr_insert=
    local _wr_cur=${COMP_WORDS[COMP_CWORD]-} _wr_candidate _wr_quoted
    local _wr_literal= _wr_quote= _wr_active_quote= _wr_joined_prefix= _wr_strip=
    local _wr_cword=$COMP_CWORD _wr_n=-1
    local _wr_offset=0 _wr_tail _wr_gap _wr_start _wr_adjacent _wr_empty
    local -a _wr_choices=() _wr_options=() _wr_commands=() _wr_candidates=()
    local -a _wr_starts=() _wr_ends=() _wr_newstarts=() _wr_newends=()
    local -a _wr_words=("${COMP_WORDS[@]}") _wr_joined=()
    COMPREPLY=()

    # Readline splits '=', ':', '@' and other word-break characters inside
    # literal argv words. Recover adjacency from source offsets without eval.
    if [[ ${COMP_LINE+x} && ${COMP_POINT+x} ]]; then
        for ((_wr_i=0; _wr_i<=COMP_CWORD; _wr_i++)); do
            _wr_word=${COMP_WORDS[_wr_i]-}
            if [[ -z $_wr_word ]]; then
                _wr_starts[_wr_i]=$COMP_POINT _wr_ends[_wr_i]=$COMP_POINT
            else
                _wr_tail=${COMP_LINE:_wr_offset}
                if [[ $_wr_tail == *"$_wr_word"* ]]; then
                    _wr_gap=${_wr_tail%%"$_wr_word"*}
                    _wr_start=$((_wr_offset + ${#_wr_gap}))
                    _wr_starts[_wr_i]=$_wr_start
                    _wr_offset=$((_wr_start + ${#_wr_word}))
                    _wr_ends[_wr_i]=$_wr_offset
                else
                    _wr_starts[_wr_i]=-1 _wr_ends[_wr_i]=-1
                fi
            fi
        done
        for ((_wr_i=0; _wr_i<=COMP_CWORD; _wr_i++)); do
            _wr_word=${COMP_WORDS[_wr_i]-}
            _wr_empty=0; [[ -z $_wr_word ]] && _wr_empty=1
            if ((_wr_i == COMP_CWORD)) && [[ ${_wr_starts[_wr_i]} -ge 0 && $COMP_POINT -ge ${_wr_starts[_wr_i]} && $COMP_POINT -lt ${_wr_ends[_wr_i]} ]]; then
                _wr_word=${_wr_word:0:COMP_POINT-${_wr_starts[_wr_i]}}
                _wr_ends[_wr_i]=$COMP_POINT
            fi
            if ((_wr_i > 0 && !_wr_empty)) && [[ ${_wr_starts[_wr_i]} -ge 0 && ${_wr_starts[_wr_i]} == "${_wr_ends[_wr_i-1]}" ]]; then
                _wr_joined[_wr_n]+=$_wr_word
                _wr_newends[_wr_n]=${_wr_ends[_wr_i]}
                if ((_wr_i == COMP_CWORD)); then
                    _wr_joined_prefix=${_wr_joined[_wr_n]:0:${#_wr_joined[_wr_n]}-${#_wr_word}}
                    # Readline preserves a delimiter when it is the current word.
                    if [[ ${#_wr_word} == 1 && ${COMP_WORDBREAKS-=:@} == *"$_wr_word"* ]]; then _wr_joined_prefix+=$_wr_word; fi
                fi
            else
                _wr_n=$((_wr_n+1)); _wr_joined[_wr_n]=$_wr_word
                _wr_newstarts[_wr_n]=${_wr_starts[_wr_i]} _wr_newends[_wr_n]=${_wr_ends[_wr_i]}
            fi
        done
        _wr_words=("${_wr_joined[@]}") _wr_cword=$_wr_n
        _wr_starts=("${_wr_newstarts[@]}") _wr_ends=("${_wr_newends[@]}")
    fi
    _wedl_rs_literal "${_wr_words[_wr_cword]-}" 1 || return 0
    _wr_cur=$_wr_literal _wr_active_quote=$_wr_quote
    if [[ -n $_wr_joined_prefix ]]; then
        _wedl_rs_literal "$_wr_joined_prefix" 1 || return 0
        _wr_strip=$_wr_literal
    fi

    for ((_wr_i=1; _wr_i<_wr_cword; _wr_i++)); do
        _wedl_rs_literal "${_wr_words[_wr_i]}" 0 || return 0
        _wr_word=$_wr_literal
        if ((_wr_pending)); then
            if [[ $_wr_word == = && ${_wr_words[_wr_i-1]} == "$_wr_pword" ]]; then
                _wr_adjacent=1
                if [[ ${_wr_starts[_wr_i]+x} ]]; then
                    [[ ${_wr_starts[_wr_i]} -ge 0 && ${_wr_starts[_wr_i]} == "${_wr_ends[_wr_i-1]}" ]] || _wr_adjacent=0
                fi
                if ((_wr_adjacent)); then
                    # '--option= ' already supplied the empty attached value.
                    _wr_empty=0
                    if [[ ${_wr_starts[_wr_i+1]+x} ]]; then
                        [[ ${_wr_starts[_wr_i+1]} -gt ${_wr_ends[_wr_i]} ]] && _wr_empty=1
                    fi
                    if ((!_wr_empty)); then continue; fi
                    _wr_word=
                fi
            fi
            if [[ $_wr_word == -* && $_wr_word != - ]] && ((!_wr_phyphen)) &&
                ! { ((_wr_pnegative)) && [[ $_wr_word =~ ^-[0-9] ]]; }; then
                ((_wr_count >= _wr_pmin)) || return 0
                _wr_pending=0
            else
                _wr_count=$((_wr_count+1))
                if ((_wr_pmax >= 0 && _wr_count >= _wr_pmax)); then _wr_pending=0; fi
                continue
            fi
        fi
        if ((_wr_enabled)) && [[ $_wr_word == -- ]]; then _wr_enabled=0; continue; fi
        if ((_wr_enabled)) && [[ $_wr_word == --* ]]; then
            _wr_name=${_wr_word%%=*} _wr_attached=0
            [[ $_wr_word == *=* ]] && _wr_attached=1
            _wedl_rs_option "$_wr_node" "$_wr_name" || return 0
            if ((_wr_max == 0)); then
                ((!_wr_attached)) || return 0
                continue
            fi
            if ((_wr_equals && !_wr_attached)); then
                [[ ${_wr_words[_wr_i+1]-} == = ]] || return 0
            fi
            _wr_pending=1 _wr_pnode=$_wr_node _wr_pword=$_wr_name _wr_count=$_wr_attached
            _wr_pmin=$_wr_min _wr_pmax=$_wr_max _wr_phyphen=$_wr_hyphen _wr_pnegative=$_wr_negative
            if ((_wr_pmax >= 0 && _wr_count >= _wr_pmax)); then _wr_pending=0; fi
            continue
        fi
        if ((_wr_enabled)) && [[ $_wr_word == -* && $_wr_word != - ]]; then
            for ((_wr_j=1; _wr_j<${#_wr_word}; _wr_j++)); do
                _wr_name=-${_wr_word:_wr_j:1}
                _wedl_rs_option "$_wr_node" "$_wr_name" || return 0
                if ((_wr_max != 0)); then
                    _wr_rest=${_wr_word:_wr_j+1} _wr_attached=0
                    [[ -n $_wr_rest ]] && _wr_attached=1
                    if ((_wr_equals)) && [[ $_wr_rest != =* ]]; then return 0; fi
                    _wr_pending=1 _wr_pnode=$_wr_node _wr_pword=$_wr_name _wr_count=$_wr_attached
                    _wr_pmin=$_wr_min _wr_pmax=$_wr_max _wr_phyphen=$_wr_hyphen _wr_pnegative=$_wr_negative
                    if ((_wr_pmax >= 0 && _wr_count >= _wr_pmax)); then _wr_pending=0; fi
                    break
                fi
            done
            continue
        fi
        if ((_wr_enabled && !_wr_seen)) && _wedl_rs_child "$_wr_node" "$_wr_word"; then
            _wr_node=$_wr_child _wr_position=0 _wr_used=0 _wr_seen=0
            continue
        fi
        _wedl_rs_position "$_wr_node" "$_wr_position" || return 0
        _wr_seen=1 _wr_used=$((_wr_used+1))
        if ((_wr_max >= 0 && _wr_used >= _wr_max)); then _wr_position=$((_wr_position+1)); _wr_used=0; fi
    done

    _wr_prefix=$_wr_cur
    if ((_wr_pending)); then
        if [[ $_wr_cur == -* && $_wr_cur != - ]] && ((!_wr_phyphen && _wr_count >= _wr_pmin)) &&
            ! { ((_wr_pnegative)) && [[ $_wr_cur =~ ^-[0-9] ]]; }; then
            _wr_pending=0
        else
            _wedl_rs_option "$_wr_pnode" "$_wr_pword" || return 0
            if [[ $_wr_cur == = && ${_wr_words[_wr_cword-1]-} == "$_wr_pword" ]]; then
                if [[ ! ${_wr_starts[_wr_cword]+x} || ${_wr_starts[_wr_cword]} == "${_wr_ends[_wr_cword-1]}" ]]; then _wr_prefix=; fi
            fi
            _wr_candidates=("${_wr_choices[@]}")
        fi
    fi
    if ((!_wr_pending)); then
        if ((_wr_enabled)) && [[ $_wr_cur == --*=* ]]; then
            _wr_name=${_wr_cur%%=*}
            _wedl_rs_option "$_wr_node" "$_wr_name" || return 0
            ((_wr_max != 0)) || return 0
            _wr_prefix=${_wr_cur#*=} _wr_insert=$_wr_name=
            _wr_candidates=("${_wr_choices[@]}")
        elif ((_wr_enabled)) && [[ $_wr_cur == -* && $_wr_cur != --* && ${#_wr_cur} -gt 2 ]]; then
            for ((_wr_j=1; _wr_j<${#_wr_cur}; _wr_j++)); do
                _wr_name=-${_wr_cur:_wr_j:1}
                _wedl_rs_option "$_wr_node" "$_wr_name" || return 0
                if ((_wr_max != 0)); then
                    _wr_prefix=${_wr_cur:_wr_j+1} _wr_insert=${_wr_cur:0:_wr_j+1}
                    if [[ $_wr_prefix == =* ]]; then _wr_prefix=${_wr_prefix#=}; _wr_insert+=\=; fi
                    if ((_wr_equals)) && [[ $_wr_insert != *= ]]; then return 0; fi
                    _wr_candidates=("${_wr_choices[@]}")
                    break
                fi
            done
        elif ((_wr_enabled)) && [[ $_wr_cur == -* ]]; then
            _wedl_rs_candidates "$_wr_node"
            _wr_candidates=("${_wr_options[@]}")
        else
            if _wedl_rs_position "$_wr_node" "$_wr_position"; then _wr_candidates=("${_wr_choices[@]}"); fi
            if ((_wr_enabled && !_wr_seen)); then
                _wedl_rs_candidates "$_wr_node"
                _wr_candidates+=("${_wr_commands[@]}")
            fi
        fi
    fi
    for _wr_candidate in "${_wr_candidates[@]}"; do
        [[ $_wr_candidate == "$_wr_prefix"* ]] || continue
        _wr_candidate=$_wr_insert$_wr_candidate
        if [[ -n $_wr_strip ]]; then
            [[ $_wr_candidate == "$_wr_strip"* ]] || continue
            _wr_candidate=${_wr_candidate:${#_wr_strip}}
        fi
        # Grammar literals are inert in the generated file and on insertion.
        if [[ $_wr_active_quote == "'" ]]; then
            _wr_quoted=${_wr_candidate//\'/\'\\\'\'}
        elif [[ $_wr_active_quote == '"' ]]; then
            _wr_quoted=${_wr_candidate//\\/\\\\}
            _wr_quoted=${_wr_quoted//\"/\\\"}
            _wr_quoted=${_wr_quoted//\$/\\\$}
            _wr_quoted=${_wr_quoted//\`/\\\`}
        else
            printf -v _wr_quoted '%q' "$_wr_candidate"
        fi
        COMPREPLY+=("$_wr_quoted")
    done
    return 0
}
complete -F _wedl_rs_completion __COMMAND__
"#;

const POWERSHELL: &str = r#"# PowerShell completion for WEDL, generated from its actual Clap command tree.
# Load this file with dot-sourcing. Tab never runs WEDL or lists authored files.
Register-ArgumentCompleter -Native -CommandName __COMMAND__, __COMMAND_EXE__ -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    $nodes = ConvertFrom-Json -InputObject __NODES__
    function Get-WedlOption($node, [string]$word) {
        foreach ($option in $nodes[$node].options) { if ($option.word -ceq $word) { return $option } }
        return $null
    }
    function Get-WedlChild($node, [string]$word) {
        foreach ($child in $nodes[$node].children) { if ($child.word -ceq $word) { return $child } }
        return $null
    }
    function Quote-WedlWord([string]$word) {
        if ($word -cmatch '^[a-zA-Z0-9_./:=\\-]+$') { return $word }
        return "'" + $word.Replace("'", "''") + "'"
    }
    function Keep-WedlWord([string]$literal, [bool]$safe, [bool]$blank) {
        # Empty callback output asks PowerShell to complete filesystem paths.
        # A public-API no-op instead retains a literal token or blank spacing.
        $text = if ($blank) { ' ' } elseif ($safe) { Quote-WedlWord $literal } else { "''" }
        [System.Management.Automation.CompletionResult]::new($text, $text, 'ParameterValue', 'No fixed completion; retain literal input')
    }
    function Read-WedlLiteral($element) {
        if ($element -is [System.Management.Automation.Language.StringConstantExpressionAst]) {
            return [pscustomobject]@{ safe = $true; value = [string]$element.Value }
        }
        # PowerShell represents a bare single-dash option as a parameter AST,
        # even for a native program. Do not evaluate parameter arguments.
        if ($element -is [System.Management.Automation.Language.CommandParameterAst] -and $null -eq $element.Argument -and $element.Extent.Text -cmatch '^-[a-zA-Z0-9_-]+$') {
            return [pscustomobject]@{ safe = $true; value = [string]$element.Extent.Text }
        }
        return [pscustomobject]@{ safe = $false; value = '' }
    }
    $previous = [System.Collections.Generic.List[string]]::new()
    $current = $null
    $valid = $true
    foreach ($element in @($commandAst.CommandElements | Select-Object -Skip 1)) {
        if ($element.Extent.StartOffset -ge $cursorPosition) { break }
        if ($element.Extent.EndOffset -ge $cursorPosition -and $wordToComplete.Length -gt 0) {
            $current = $element
            break
        }
        $read = Read-WedlLiteral $element
        # Still find the current literal after an earlier expression. Its text
        # must survive the no-op even though command traversal is unavailable.
        if (-not $read.safe) { $valid = $false; continue }
        $previous.Add($read.value)
    }
    $blank = $wordToComplete.Length -eq 0
    $read = Read-WedlLiteral $current
    $safe = $blank -or $read.safe
    $literal = if ($blank) { '' } else { $read.value }
    if (-not $valid -or -not $safe) { Keep-WedlWord $literal $safe $blank; return }
    # At a cursor inside a token, parse the prefix as syntax only. Never invoke
    # an expression, variable, command substitution or SafeGetValue evaluator.
    if ($null -ne $current -and $current.Extent.EndOffset -gt $cursorPosition) {
        $prefixText = $current.Extent.Text.Substring(0, $cursorPosition - $current.Extent.StartOffset)
        $prefixTokens = $null; $prefixErrors = $null
        $prefixAst = [System.Management.Automation.Language.Parser]::ParseInput('wedl-rs ' + $prefixText, [ref]$prefixTokens, [ref]$prefixErrors)
        $prefixCommand = $prefixAst.Find({ param($item) $item -is [System.Management.Automation.Language.CommandAst] }, $false)
        $prefixElements = @($prefixCommand.CommandElements)
        $read = if ($prefixElements.Count -eq 2) { Read-WedlLiteral $prefixElements[1] } else { [pscustomobject]@{ safe = $false; value = '' } }
        if (-not $read.safe) {
            Keep-WedlWord '' $false $false; return
        }
        $literal = $read.value
    }

    $node = 0; $position = 0; $used = 0; $seen = $false; $enabled = $true
    $pending = $null; $count = 0
    foreach ($word in $previous) {
        if ($null -ne $pending) {
            $boundary = $word.StartsWith('-',[System.StringComparison]::Ordinal) -and $word -cne '-' -and -not $pending.hyphen -and -not ($pending.negative -and $word -cmatch '^-[0-9]')
            if ($boundary) {
                if ($count -lt $pending.min) { Keep-WedlWord $literal $true $blank; return }
                $pending = $null
            } else {
                $count++
                if ($pending.max -ge 0 -and $count -ge $pending.max) { $pending = $null }
                continue
            }
        }
        if ($enabled -and $word -ceq '--') { $enabled = $false; continue }
        if ($enabled -and $word.StartsWith('--',[System.StringComparison]::Ordinal)) {
            $equal = $word.IndexOf('=')
            $name = if ($equal -ge 0) { $word.Substring(0,$equal) } else { $word }
            $option = Get-WedlOption $node $name
            if ($null -eq $option -or ($option.max -eq 0 -and $equal -ge 0)) { Keep-WedlWord $literal $true $blank; return }
            if ($option.max -eq 0) { continue }
            if ($option.equals -and $equal -lt 0) { Keep-WedlWord $literal $true $blank; return }
            $pending = $option; $count = if ($equal -ge 0) { 1 } else { 0 }
            if ($pending.max -ge 0 -and $count -ge $pending.max) { $pending = $null }
            continue
        }
        if ($enabled -and $word.StartsWith('-',[System.StringComparison]::Ordinal) -and $word -cne '-') {
            for ($i=1; $i -lt $word.Length; $i++) {
                $option = Get-WedlOption $node ('-' + $word[$i])
                if ($null -eq $option) { Keep-WedlWord $literal $true $blank; return }
                if ($option.max -ne 0) {
                    $rest = $word.Substring($i+1)
                    if ($option.equals -and -not $rest.StartsWith('=')) { Keep-WedlWord $literal $true $blank; return }
                    $pending = $option; $count = if ($rest.Length -gt 0) { 1 } else { 0 }
                    if ($pending.max -ge 0 -and $count -ge $pending.max) { $pending = $null }
                    break
                }
            }
            continue
        }
        if ($enabled -and -not $seen) {
            $child = Get-WedlChild $node $word
            if ($null -ne $child) { $node = $child.node; $position = 0; $used = 0; $seen = $false; continue }
        }
        if ($position -ge @($nodes[$node].positions).Count) { Keep-WedlWord $literal $true $blank; return }
        $argument = $nodes[$node].positions[$position]
        $seen = $true; $used++
        if ($argument.max -ge 0 -and $used -ge $argument.max) { $position++; $used = 0 }
    }

    $prefix = $literal; $insert = ''; $candidates = @()
    if ($null -ne $pending) {
        $boundary = $literal.StartsWith('-',[System.StringComparison]::Ordinal) -and $literal -cne '-' -and -not $pending.hyphen -and $count -ge $pending.min -and -not ($pending.negative -and $literal -cmatch '^-[0-9]')
        if ($boundary) { $pending = $null } else { $candidates = @($pending.choices) }
    }
    if ($null -eq $pending) {
        if ($enabled -and $literal.StartsWith('--',[System.StringComparison]::Ordinal) -and $literal.Contains('=')) {
            $equal = $literal.IndexOf('='); $name = $literal.Substring(0,$equal)
            $option = Get-WedlOption $node $name
            if ($null -eq $option -or $option.max -eq 0) { Keep-WedlWord $literal $true $blank; return }
            $prefix = $literal.Substring($equal+1); $insert = $name + '='; $candidates = @($option.choices)
        } elseif ($enabled -and $literal.StartsWith('-',[System.StringComparison]::Ordinal) -and -not $literal.StartsWith('--',[System.StringComparison]::Ordinal) -and $literal.Length -gt 2) {
            for ($i=1; $i -lt $literal.Length; $i++) {
                $option = Get-WedlOption $node ('-' + $literal[$i])
                if ($null -eq $option) { Keep-WedlWord $literal $true $blank; return }
                if ($option.max -ne 0) {
                    $prefix = $literal.Substring($i+1); $insert = $literal.Substring(0,$i+1)
                    if ($prefix.StartsWith('=')) { $prefix = $prefix.Substring(1); $insert += '=' }
                    if ($option.equals -and -not $insert.EndsWith('=')) { Keep-WedlWord $literal $true $blank; return }
                    $candidates = @($option.choices); break
                }
            }
        } elseif ($enabled -and $literal.StartsWith('-',[System.StringComparison]::Ordinal)) {
            $candidates = @($nodes[$node].options | Where-Object { $_.visible } | ForEach-Object { $_.word })
        } else {
            if ($position -lt @($nodes[$node].positions).Count) { $candidates = @($nodes[$node].positions[$position].choices) }
            if ($enabled -and -not $seen) { $candidates += @($nodes[$node].children | Where-Object { $_.visible } | ForEach-Object { $_.word }) }
        }
    }
    $matches = 0
    foreach ($candidate in $candidates) {
        if (-not $candidate.StartsWith($prefix,[System.StringComparison]::Ordinal)) { continue }
        $value = $insert + $candidate; $text = Quote-WedlWord $value; $matches++
        [System.Management.Automation.CompletionResult]::new($text, $value, 'ParameterValue', $value)
    }
    if ($matches -eq 0) { Keep-WedlWord $literal $true $blank }
}
"#;
