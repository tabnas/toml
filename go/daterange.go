// Copyright (c) 2021-2026 Richard Rodger and other contributors, MIT License

package tabnastoml

import (
	"regexp"
	"strconv"
)

// Date and time RANGE validation.
//
// Both ports matched date/time values on SHAPE alone — `^\d\d\d\d-\d\d-\d\d`
// says nothing about whether 13 is a month or 32 a day. Neither port noticed,
// and each mishandled the result in its own way. This one kept the source
// text, so it round-tripped the impossible date back out unchanged; the
// TypeScript port built a JS Date, which never fails, so 1988-02-30 rolled
// over to 1988-03-01 and 2006-01-32 became an Invalid Date serializing to
// null — the value destroyed outright. Twelve invalid documents, accepted by
// both, with 38 value disagreements between them.
//
// Range-checking here is what makes the two ports agree, because it removes
// the value they were disagreeing about rather than choosing between them.

var monthDays = [12]int{31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31}

func daysInMonth(year, month int) int {
	if 2 != month {
		return monthDays[month-1]
	}
	// Proleptic Gregorian, the calendar RFC 3339 specifies. 2100 is NOT a
	// leap year, which is exactly what the corpus's feb-29 document tests.
	leap := (0 == year%4 && 0 != year%100) || 0 == year%400
	if leap {
		return 29
	}
	return 28
}

// Seconds may be 60: RFC 3339 permits a positive leap second, and TOML
// inherits its date-time grammar from it. 61 is the corpus's second-over.
func timeInRange(hour, minute, second int) bool {
	return hour <= 23 && minute <= 59 && second <= 60
}

// The capture forms of isodateRe and localtimeRe. Anchored at BOTH ends so
// they can only agree with what those regexps already matched; a mismatch
// means the two have drifted apart, and the value is let through rather than
// silently rejected on a shape this code does not actually understand.
var (
	isodateParts = regexp.MustCompile(
		`^(\d{4})-(\d{2})-(\d{2})(?:[Tt ](\d{2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?` +
			`(?:[Zz]|[-+](\d{2}):(\d{2}))?)?$`,
	)
	localtimeParts = regexp.MustCompile(
		`^(\d{2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?$`,
	)
)

// num converts a capture group that the regexp has already constrained to
// digits. An empty group is absent, reported as -1 so callers can tell it
// apart from a real zero.
func num(s string) int {
	if "" == s {
		return -1
	}
	n, err := strconv.Atoi(s)
	if nil != err {
		return -1
	}
	return n
}

func isodateInRange(text string) bool {
	p := isodateParts.FindStringSubmatch(text)
	if nil == p {
		return true
	}

	year, month, day := num(p[1]), num(p[2]), num(p[3])
	if month < 1 || 12 < month || day < 1 || daysInMonth(year, month) < day {
		return false
	}

	// No time part: the date alone is in range.
	if "" == p[4] {
		return true
	}
	second := num(p[6])
	if second < 0 {
		second = 0
	}
	if !timeInRange(num(p[4]), num(p[5]), second) {
		return false
	}

	// Offset, when written as +hh:mm rather than Z.
	return "" == p[7] || (num(p[7]) <= 23 && num(p[8]) <= 59)
}

func localtimeInRange(text string) bool {
	p := localtimeParts.FindStringSubmatch(text)
	if nil == p {
		return true
	}
	second := num(p[3])
	if second < 0 {
		second = 0
	}
	return timeInRange(num(p[1]), num(p[2]), second)
}
