// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

package tabnastoml

import (
	"reflect"
	"testing"
)

func TestParseHappy(t *testing.T) {
	result, err := Parse("a=1")
	if err != nil {
		t.Fatalf("Parse failed: %v", err)
	}
	m, ok := result.(map[string]any)
	if !ok {
		t.Fatalf("expected map, got %T: %v", result, result)
	}
	if !reflect.DeepEqual(m["a"], float64(1)) && !reflect.DeepEqual(m["a"], 1) {
		t.Fatalf("expected a=1, got a=%v (%T)", m["a"], m["a"])
	}
}

func TestParseEmpty(t *testing.T) {
	result, err := Parse("")
	if err != nil {
		t.Fatalf("Parse failed: %v", err)
	}
	m, ok := result.(map[string]any)
	if !ok {
		t.Fatalf("expected map, got %T: %v", result, result)
	}
	if len(m) != 0 {
		t.Fatalf("expected empty map, got %v", m)
	}
}

func TestVersion(t *testing.T) {
	if Version == "" {
		t.Fatal("Version should be set")
	}
}
