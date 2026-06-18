// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

package tabnastoml

// TomlTime captures a TOML date / time / datetime value alongside its
// original source text and kind tag, matching the metadata the
// TypeScript port attaches to JavaScript Date objects as `__toml__`.
type TomlTime struct {
	Kind string // "offset-date-time", "local-date-time", "local-date", "local-time"
	Src  string // original source text
}

// isodateVal resolves the @isodate-val grammar ref. It inspects the
// regex match groups to decide whether the value is a local/offset
// date or date-time.
func isodateVal(res []string) any {
	kind := "local"
	// res[4] is the timezone suffix group (Z or +hh:mm); if present → offset.
	if len(res) > 4 && res[4] != "" {
		kind = "offset"
	}
	kind += "-date"
	// res[1] is the "Thh:mm..." group; if present → includes time.
	if len(res) > 1 && res[1] != "" {
		kind += "-time"
	}
	return &TomlTime{Kind: kind, Src: res[0]}
}

// localtimeVal resolves the @localtime-val grammar ref.
func localtimeVal(res []string) any {
	return &TomlTime{Kind: "local-time", Src: res[0]}
}
