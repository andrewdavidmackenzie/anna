package tests

import (
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

func annaBinary() string {
	root := filepath.Join("..", "..", "..")
	return filepath.Join(root, "target", "anna-go")
}

func normalizeSetLine(line string) string {
	line = strings.TrimSpace(line)
	if !strings.HasPrefix(line, "{") || !strings.HasSuffix(line, "}") {
		return line
	}
	inner := strings.TrimSpace(line[1 : len(line)-1])
	parts := strings.Fields(inner)
	sort.Strings(parts)
	return "{ " + strings.Join(parts, " ") + " }"
}

func TestCLISmokeTest(t *testing.T) {
	startServers(t)
	defer stopServers()

	binary := annaBinary()
	if _, err := os.Stat(binary); os.IsNotExist(err) {
		t.Skip("anna-go binary not found, run 'make client-go' first")
	}

	config := configFile()
	inputFile := filepath.Join("cli", "input")
	expectedFile := filepath.Join("cli", "expected")

	cmd := exec.Command(binary, "--config", config, "cli", inputFile)
	cmd.Env = append(os.Environ(), "PATH="+os.Getenv("PATH")+":"+serverBinaryDir())
	out, err := cmd.Output()
	if err != nil {
		if exitErr, ok := err.(*exec.ExitError); ok {
			t.Fatalf("anna-go cli failed: %v\nstderr: %s", err, exitErr.Stderr)
		}
		t.Fatalf("anna-go cli failed: %v", err)
	}

	expectedBytes, err := os.ReadFile(expectedFile)
	if err != nil {
		t.Fatalf("Failed to read expected file: %v", err)
	}

	gotLines := strings.Split(strings.TrimSpace(string(out)), "\n")
	expectedLines := strings.Split(strings.TrimSpace(string(expectedBytes)), "\n")

	if len(gotLines) != len(expectedLines) {
		t.Fatalf("Line count mismatch: got %d, expected %d\ngot:\n%s\nexpected:\n%s",
			len(gotLines), len(expectedLines), string(out), string(expectedBytes))
	}

	for i := range gotLines {
		got := normalizeSetLine(gotLines[i])
		expected := normalizeSetLine(expectedLines[i])
		if got != expected {
			t.Errorf("Line %d mismatch:\n  got:      %q\n  expected: %q", i+1, got, expected)
		}
	}
}
