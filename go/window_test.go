// Copyright (c) 2026 Richard Rodger and other contributors, MIT License

package tabnastoml

// window_test.go — a parse from a reader (Tabnas.ParseReader, admin ADR-30
// and ADR-33).
//
// MakeJsonic builds the parser whole, as jsonic.Make() with the TOML
// grammar installed, and declares lex.window on it, so that a reader is
// read a window at a time. The plugin's own matchers, its strings, its
// dates and times and the leading byte order mark, read the source through
// the window and read on where a scan reaches its end. These tests hold it
// to that: MakeJsonic declares the window and parses every fixture row as
// the parser without it does; every row, and every document of the
// toml-test corpus, both halves, parses the same from a reader, in pieces
// of 1, 3, 64 and 4096 bytes, as from its text, but for the corpus
// documents that are not UTF-8, which a reader refuses as unreadable; and
// tokens longer than the window, dates whose offset straddles the window's
// end, and a long document parse the same too.
//
// Go's window sizes are fixed at 64 KiB (the engine's small-window hook is
// unexported), and before each token the window reaches 64 KiB past the
// cursor, so a token meets the window's edge only where it is longer than
// that: the long tokens and the long document carry such tokens of every
// kind the plugin cuts.

import (
	"errors"
	"fmt"
	"io"
	"math"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"testing"
	"unicode/utf8"

	jsonic "github.com/tabnas/jsonic/go"
	tabnas "github.com/tabnas/parser/go"
	support "github.com/tabnas/support/go"
)

// readerMargin is how far past the cursor the window reaches before each
// token is cut: a token longer than this meets the window's edge.
const readerMargin = 64 << 10

// pieces is a reader that hands its bytes over at most size at a time.
type pieces struct {
	text []byte
	at   int
	size int
}

func (p *pieces) Read(buf []byte) (int, error) {
	if p.at == len(p.text) {
		return 0, io.EOF
	}
	n := min(p.size, len(buf), len(p.text)-p.at)
	copy(buf, p.text[p.at:p.at+n])
	p.at += n
	return n, nil
}

// canonical is a value as text that tells apart everything a parse can
// give: the order of a table's keys, a number's sign and NaN (which
// reflect.DeepEqual calls unequal to itself), and a date or time by its
// kind and source.
func canonical(v any) string {
	switch x := v.(type) {
	case *tabnas.OrderedMap:
		parts := make([]string, 0, len(x.Keys))
		for _, k := range x.Keys {
			parts = append(parts, strconv.Quote(k)+":"+canonical(x.Vals[k]))
		}
		return "{" + strings.Join(parts, ",") + "}"
	case map[string]any:
		keys := make([]string, 0, len(x))
		for k := range x {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		parts := make([]string, 0, len(keys))
		for _, k := range keys {
			parts = append(parts, strconv.Quote(k)+":"+canonical(x[k]))
		}
		return "map{" + strings.Join(parts, ",") + "}"
	case []any:
		parts := make([]string, 0, len(x))
		for _, e := range x {
			parts = append(parts, canonical(e))
		}
		return "[" + strings.Join(parts, ",") + "]"
	case *TomlTime:
		return "time(" + x.Kind + " " + strconv.Quote(x.Src) + ")"
	case float64:
		if x == 0 && math.Signbit(x) {
			return "-0"
		}
		return strconv.FormatFloat(x, 'g', -1, 64)
	case string:
		return strconv.Quote(x)
	}
	return fmt.Sprintf("%T(%#v)", v, v)
}

// shown is what a parse shows: its value, or its error's code, position,
// row, column and token text. An error's excerpt is left out: a parse from
// a reader shows a long line only as far back as the window reaches
// (tabnas/parser DIVERGENCE.md), and that is the one difference allowed.
func shown(v any, err error) string {
	if err == nil {
		return "value " + canonical(v)
	}
	var te *tabnas.TabnasError
	if errors.As(err, &te) {
		return fmt.Sprintf("error %s at %d %d:%d %q", te.Code, te.Pos, te.Row, te.Col, te.Src)
	}
	return "not a TabnasError: " + err.Error()
}

func clip(s string) string {
	if len(s) > 300 {
		return s[:300] + "..."
	}
	return s
}

// sameFromReader parses text whole and from a reader in pieces of each
// size, and reports where the two differ. It returns the number of reader
// parses compared.
func sameFromReader(t *testing.T, j *tabnas.Tabnas, at, text string, sizes []int) int {
	t.Helper()
	whole := shown(j.Parse(text))
	for _, size := range sizes {
		read := shown(j.ParseReader(&pieces{text: []byte(text), size: size}))
		if read != whole {
			t.Errorf("%s: %q in pieces of %d:\n  whole:  %s\n  reader: %s",
				at, clip(text), size, clip(whole), clip(read))
		}
	}
	return len(sizes)
}

// readerSizes are the pieces a document is handed over in: down to a byte
// for a short one, and from 64 bytes for one over 8 KiB, whose every
// one-byte read the Go window answers with a copy of itself
// (tabnas/parser#321).
func readerSizes(text string) []int {
	if len(text) > 8<<10 {
		return []int{64, 4096, readerMargin + 3}
	}
	return []int{1, 3, 64, 4096}
}

// fixtureRows calls each with every row of the shared fixtures: where it
// is, and its input.
func fixtureRows(t *testing.T, each func(at, input string)) {
	t.Helper()
	dir, err := support.FindSpecDir("")
	if err != nil {
		t.Fatal(err)
	}
	paths, err := filepath.Glob(filepath.Join(dir, "*.tsv"))
	if err != nil || len(paths) == 0 {
		t.Fatalf("no fixtures in %s: %v", dir, err)
	}
	for _, path := range paths {
		spec, err := support.LoadSpec(path, nil)
		if err != nil {
			t.Fatal(err)
		}
		for _, row := range spec.Rows {
			each(fmt.Sprintf("%s:%d", filepath.Base(path), row.Line), row.UnescNamed("input"))
		}
	}
}

// withoutWindow is the parser MakeJsonic built before it declared the
// window, and still builds but for that: jsonic's base with the grammar
// installed.
func withoutWindow(t *testing.T) *tabnas.Tabnas {
	t.Helper()
	j := jsonic.Make()
	if err := apply(j); err != nil {
		t.Fatal(err)
	}
	return j
}

// TestMakeJsonicDeclaresTheWindow: MakeJsonic declares lex.window, and
// jsonic's base with the grammar installed does not.
func TestMakeJsonicDeclaresTheWindow(t *testing.T) {
	if !MakeJsonic().Config().Window {
		t.Error("MakeJsonic does not declare lex.window")
	}
	if withoutWindow(t).Config().Window {
		t.Error("installing the grammar declares lex.window")
	}
}

// TestMakeJsonicParsesAsBefore: every fixture row parses through
// MakeJsonic as through the parser without the window.
func TestMakeJsonicParsesAsBefore(t *testing.T) {
	made, before := MakeJsonic(), withoutWindow(t)
	n := 0
	fixtureRows(t, func(at, input string) {
		a, b := shown(made.Parse(input)), shown(before.Parse(input))
		if a != b {
			t.Errorf("%s:\n  MakeJsonic: %s\n  before:     %s", at, a, b)
		}
		n++
	})
	if n < 200 {
		t.Errorf("only %d rows", n)
	}
}

// TestFixturesFromAReader: every fixture row parses the same from a reader
// as from its text, under MakeJsonic.
func TestFixturesFromAReader(t *testing.T) {
	compared := 0
	fixtureRows(t, func(at, input string) {
		compared += sameFromReader(t, MakeJsonic(), at, input, readerSizes(input))
	})
	t.Logf("reader fixtures: %d reader parses (100%%)", compared)
	if compared < 800 {
		t.Errorf("only %d reader parses", compared)
	}
}

// TestCorpusFromAReader: every toml-test document that is UTF-8, in both
// halves, parses the same from a reader as from its text; one that is not,
// a reader refuses as unreadable.
func TestCorpusFromAReader(t *testing.T) {
	j := MakeJsonic()
	halves := []string{"valid", "invalid"}
	compared, refused := 0, 0
	for n, half := range halves {
		root := filepath.Join(ensureCorpus(t), "tests", half)
		var files []string
		err := filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
			if err == nil && !info.IsDir() && strings.HasSuffix(path, ".toml") {
				files = append(files, path)
			}
			return err
		})
		if err != nil {
			t.Fatal(err)
		}
		sort.Strings(files)
		for _, path := range files {
			b, err := os.ReadFile(path)
			if err != nil {
				t.Fatal(err)
			}
			rel, _ := filepath.Rel(root, path)
			at := half + "/" + filepath.ToSlash(rel)
			if !utf8.Valid(b) {
				refused++
				read := shown(j.ParseReader(&pieces{text: b, size: 3}))
				if !strings.HasPrefix(read, "error unreadable ") {
					t.Errorf("%s: from a reader %s, not unreadable", at, read)
				}
				continue
			}
			text := string(b)
			compared += sameFromReader(t, j, at, text, readerSizes(text))
		}
		t.Logf("reader corpus: %d of %d halves (%d%%), %d reader parses",
			n+1, len(halves), (n+1)*100/len(halves), compared)
	}
	// The nine documents of invalid/encoding that are not UTF-8.
	if refused != 9 {
		t.Errorf("%d documents refused as not UTF-8, want 9", refused)
	}
	if compared < 3000 {
		t.Errorf("only %d reader parses", compared)
	}
}

// longTokens are documents whose tokens are longer than the window: each
// kind of token the plugin cuts itself, and the engine's around them, long,
// and cut short or refused past the window's edge. Their repeated parts
// (escapes, quotes, line-ending backslashes) put one across the edge
// wherever it falls.
func longTokens() []string {
	long := readerMargin + 2<<10
	return []string{
		"a = \"" + strings.Repeat("s\\u00e9\\U0001F600\\x41\\n", long/24) + "\"\nb = 1",
		"a = \"" + strings.Repeat("é", long) + "\"",
		"a = '" + strings.Repeat("l", long) + "'",
		"a = \"\"\"\n" + strings.Repeat("m\\n\"\"x", long/6) + "\"\"\"\"\"",
		"a = \"\"\"" + strings.Repeat("w\\\n \n\t", long/6) + "z\"\"\"",
		"a = '''" + strings.Repeat("q''\n", long/4) + "''''",
		"a = 1979-05-27T07:32:00." + strings.Repeat("9", long) + "+07:00\nb = 2",
		"a = 07:32:00." + strings.Repeat("5", long) + "\nb = 2",
		"a = 1979-05-27 07:32:00." + strings.Repeat("1", long) + "Z",
		"k" + strings.Repeat("e", long) + "y = 1\n\"" + strings.Repeat("q", long) + "\" = 2",
		"# " + strings.Repeat("c", long) + "\na = 1",
		"a = 1" + strings.Repeat(" ", long) + "\n" + strings.Repeat("\n", long) + "b = 2",
		"a = 1" + strings.Repeat("0", long),
		"a = 1." + strings.Repeat("5", long) + "e3",
		"a = [" + strings.Repeat("1, ", long/3) + "2]",
		"a = \"" + strings.Repeat("u", long),
		"a = \"\"\"" + strings.Repeat("u\n", long/2),
		"a = \"" + strings.Repeat("p", long) + "\x01\"",
		"a = \"" + strings.Repeat("x", long) + "\\x4\"",
		"a = \"" + strings.Repeat("x", long) + "\\u00",
		"a = 1979-05-27T07:32:00." + strings.Repeat("9", long) + "+07:",
		"a = 2006-01-32T07:32:00." + strings.Repeat("9", long),
	}
}

// TestLongTokensFromAReader: tokens longer than the window parse the same
// from a reader as from their text.
func TestLongTokensFromAReader(t *testing.T) {
	j := MakeJsonic()
	docs := longTokens()
	for n, doc := range docs {
		sameFromReader(t, j, fmt.Sprintf("long token %d", n), doc, []int{64, 4096, readerMargin + 3})
		t.Logf("reader long tokens: %d of %d (%d%%)", n+1, len(docs), (n+1)*100/len(docs))
	}
}

// TestDateOffsetAcrossTheEdgeFromAReader: a date whose fraction ends just
// before the window's end, its `+hh:mm` offset across it. The date pattern
// reads up to six bytes past what it matches, which the window's margin
// covers for any date shorter than the margin; a fraction this long puts
// the read past the window, and the match must still take the offset. The
// date starts at byte 4 and its fraction at byte 24, and the window it is
// cut from ends at the first read boundary 64 KiB past it: byte 65,600 for
// reads of 64 bytes and 69,632 for reads of 4096. A fraction of 65,571 to
// 65,575 digits, or of 69,603 to 69,607, ends one to five bytes before
// that, and the spans below hold those with a margin either side.
func TestDateOffsetAcrossTheEdgeFromAReader(t *testing.T) {
	j := MakeJsonic()
	spans := [][2]int{{65560, 65590}, {69590, 69620}}
	total := 0
	for _, span := range spans {
		total += span[1] - span[0]
	}
	done := 0
	for _, span := range spans {
		for digits := span[0]; digits < span[1]; digits++ {
			doc := "a = 1979-05-27T07:32:00." + strings.Repeat("9", digits) + "+07:00\nb = 1\n"
			sameFromReader(t, j, fmt.Sprintf("a fraction of %d digits", digits), doc, []int{64, 4096})
			done++
		}
		t.Logf("reader date offsets: %d of %d (%d%%)", done, total, done*100/total)
	}
}

// longDocument is a document of records arrays-of-tables elements, each
// with a string, a literal string, dates and times, a float, an array
// holding an inline table, a multi-line string with a line-ending
// backslash and a dotted key, with every long token of longTokens that
// parses at every thousandth.
func longDocument(records int) string {
	var b strings.Builder
	b.WriteString("# records\ntitle = \"records\"\n")
	for i := 0; i < records; i++ {
		fmt.Fprintf(&b, "\n[[record]]\nid = %d\nname = \"ñ%d\\u00e9\" # note %d\n"+
			"path = 'C:\\dir\\%d'\nwhen = 1979-05-27T07:32:%02d.%d-07:00\n"+
			"at = 07:32:00.5\nday = 2026-11-09\nscore = -1.5e3\n"+
			"tags = [\"a\", \"b\", { k = %d }]\n"+
			"note = \"\"\"\nline %d\\\n  joined\"\"\"\ndeep.er.key = true\n",
			i, i, i, i, i%60, i, i, i)
		if i%1000 == 999 {
			long := readerMargin + 2<<10
			fmt.Fprintf(&b, "s%d = \"%s\"\n", i, strings.Repeat("s\\u00e9\\x41\\n", long/14))
			fmt.Fprintf(&b, "l%d = '%s'\n", i, strings.Repeat("l", long))
			fmt.Fprintf(&b, "m%d = \"\"\"%s\"\"\"\n", i, strings.Repeat("m\\\n \"\"", long/7))
			fmt.Fprintf(&b, "q%d = '''%s'''\n", i, strings.Repeat("q''\n", long/4))
			fmt.Fprintf(&b, "d%d = 1979-05-27T07:32:00.%s+07:00\n", i, strings.Repeat("9", long))
			fmt.Fprintf(&b, "t%d = 07:32:00.%s\n", i, strings.Repeat("5", long))
			fmt.Fprintf(&b, "# %s\n%s\n%s", strings.Repeat("c", long), strings.Repeat(" ", long), strings.Repeat("\n", long))
			fmt.Fprintf(&b, "k%s%d = 1\n", strings.Repeat("e", long), i)
		}
	}
	return b.String()
}

// TestLongDocumentFromAReader: a document of over a megabyte, its tokens
// of every kind long and short, the long ones longer than the window,
// parses the same from a reader as from its text, and so does the same
// document with an error at its end, which a parse from a reader places
// where the whole text does after the window has moved on many times.
func TestLongDocumentFromAReader(t *testing.T) {
	body := longDocument(3000)
	text := body + "\n[end]\nok = true\n"
	broken := body + "\n[end]\nok = \"tail\x01\"\n"
	if len(text) < 1<<20 {
		t.Fatalf("the document is %d bytes", len(text))
	}
	j := MakeJsonic()
	if _, err := j.Parse(text); err != nil {
		t.Fatalf("the long document does not parse: %v", err)
	}
	if _, err := j.Parse(broken); err == nil || err.(*tabnas.TabnasError).Code != "unprintable" {
		t.Fatalf("the broken document is not unprintable: %v", err)
	}
	sameFromReader(t, j, "the long document", text, []int{64, 4096, readerMargin + 3})
	t.Logf("reader long document: 1 of 2 (50%%), %d bytes", len(text))
	sameFromReader(t, j, "the long document with an error at its end", broken, []int{4096, readerMargin + 3})
	t.Logf("reader long document: 2 of 2 (100%%), %d bytes", len(broken))
}
