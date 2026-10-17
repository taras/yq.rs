# Conformance fixture

`scenarios.json` holds every `expressionScenario` from the Go yq test suite,
dumped verbatim by `scripts/extract-scenarios.sh`. It is the specification
yq.rs is built against. Do not edit it by hand; regenerate it from a newer yq
if needed and record the commit here.

| | |
|---|---|
| Source | https://github.com/mikefarah/yq |
| Commit | `a814ff5166040ec745a96bddd79ee31c3a548e83` (2026-10-09) |
| Scenarios | 1121 in 76 groups |

## Scenario fields

| Field | Meaning |
|---|---|
| `group` | Name of the Go slice the scenario came from, e.g. `addOperatorScenarios`. Maps to `operator_add_test.go` and `operator_add.go`. |
| `description`, `subdescription` | Human text, also used for yq's generated docs. |
| `document` | Input YAML. Empty means the input is a single `!!null` scalar node (document 0, no filename). Parsed as file `sample.yml`, file index 0. |
| `document2` | Optional second input, parsed as `another.yml`, file index 1, appended to the same input list. |
| `expression` | The yq expression under test. |
| `expected` | One string per result node, in the result format below. Order matters. |
| `expectedError` | When set, evaluation must fail and the error message must contain this text. `expected` is then empty. |
| `environmentVariables` | Set before evaluation; read by `env`, `strenv`, `envsubst`. |
| `requiresFormat` | `xml` or `json`: needs a non-YAML decoder. The harness skips these. |
| `skipDoc`, `skipForGoccy` | Only relevant to yq's own doc generation and alternate parser. Ignore. |

## Result format

Each expected string is produced by `resultToString` in
`pkg/yqlib/operators_test.go`:

```
D<document index>, P<path>, (<tag>)::<printed node>
```

- `<document index>` is the node's document index (0-based, across both
  input files).
- `<path>` is Go's `%v` of the node's path slice: elements separated by a
  single space inside square brackets, map keys as bare strings, sequence
  indices as integers. Root is `[]`. Example: `[a b 0]`.
- `<tag>` is the node's tag, e.g. `!!map`, `!!str`, `!!int`. Alias nodes
  print `alias`.
- `<printed node>` is the node printed by the YAML encoder with **indent 4**,
  unwrapScalar on (scalars print their value without quotes or comments) and
  document separators on. It ends with a newline. Study
  `pkg/yqlib/printer.go` and `encoder_yaml.go` for the exact rules.

## Fixed clock

The Go suite pins `Now` to `2021-05-19T01:02:03.000000004Z` (see
`TestMain`). The conformance runner must evaluate with that timestamp so the
`now` and date scenarios match.

## Groups

| Group | Scenarios |
|---|---|
| addOperatorScenarios | 65 |
| alternativeOperatorScenarios | 12 |
| anchorOperatorScenarios | 31 |
| arrayToMapScenarios | 1 |
| assignOperatorScenarios | 28 |
| badAnchorOperatorScenarios | 6 |
| badTraversePathOperatorScenarios | 5 |
| booleanOperatorScenarios | 33 |
| collectObjectOperatorScenarios | 23 |
| collectOperatorScenarios | 16 |
| columnOperatorScenarios | 4 |
| commentOperatorScenarios | 29 |
| compareOperatorScenarios | 44 |
| containsOperatorScenarios | 11 |
| createMapOperatorScenarios | 13 |
| dateTimeOperatorScenarios | 14 |
| deleteOperatorScenarios | 31 |
| divideOperatorScenarios | 14 |
| documentIndexScenarios | 5 |
| encoderDecoderOperatorScenarios | 41 |
| entriesOperatorScenarios | 15 |
| envOperatorScenarios | 21 |
| envOperatorSecurityDisabledScenarios | 3 |
| equalsOperatorScenarios | 22 |
| errorOperatorScenarios | 3 |
| evalOperatorScenarios | 3 |
| evaluateNodesScenario | 3 |
| fileOperatorScenarios | 5 |
| filterOperatorScenarios | 6 |
| firstOperatorScenarios | 22 |
| fixedAnchorOperatorScenarios | 16 |
| fixedTraversePathOperatorScenarios | 5 |
| flattenOperatorScenarios | 6 |
| groupByOperatorScenarios | 3 |
| hasOperatorScenarios | 7 |
| ifOperatorScenarios | 20 |
| keysOperatorScenarios | 11 |
| kindOperatorScenarios | 3 |
| lengthOperatorScenarios | 6 |
| lineOperatorScenarios | 4 |
| loadOperatorSecurityDisabledScenarios | 5 |
| loadScenarios | 14 |
| mapOperatorScenarios | 13 |
| maxOperatorScenarios | 3 |
| minOperatorScenarios | 3 |
| moduloOperatorScenarios | 13 |
| multiplyOperatorScenarios | 78 |
| omitOperatorScenarios | 6 |
| parentOperatorScenarios | 14 |
| pathOperatorScenarios | 16 |
| pickOperatorScenarios | 7 |
| pipeOperatorScenarios | 3 |
| pivotOperatorScenarios | 5 |
| recipes | 8 |
| recursiveDescentOperatorScenarios | 22 |
| reduceOperatorScenarios | 4 |
| reverseOperatorScenarios | 7 |
| selectOperatorScenarios | 18 |
| shuffleOperatorScenarios | 3 |
| sliceArrayScenarios | 23 |
| sortByOperatorScenarios | 20 |
| sortKeysOperatorScenarios | 4 |
| splitDocOperatorScenarios | 3 |
| stringsOperatorScenarios | 43 |
| styleOperatorScenarios | 17 |
| subtractOperatorScenarios | 19 |
| systemOperatorDisabledScenarios | 1 |
| tagOperatorScenarios | 10 |
| toNumberScenarios | 4 |
| traversePathOperatorScenarios | 64 |
| unionOperatorScenarios | 5 |
| uniqueOperatorScenarios | 11 |
| valueOperatorScenarios | 26 |
| variableOperatorScenarios | 10 |
| withOperatorScenarios | 5 |
| yamlParseScenarios | 4 |
