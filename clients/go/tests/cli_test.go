package tests

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func annaBinary() string {
	root := filepath.Join("..", "..", "..")
	return filepath.Join(root, "target", "anna-go")
}

func TestCLISmokeTest(t *testing.T) {
	binary := annaBinary()
	if _, err := os.Stat(binary); os.IsNotExist(err) {
		t.Skip("anna-go binary not found, run 'make client-go' first")
	}

	root := filepath.Join("..", "..", "..")
	runner := filepath.Join(root, "tests", "shared", "cli", "run_smoke_test.py")

	absBinary, _ := filepath.Abs(binary)
	cmd := exec.Command("python3", runner,
		absBinary, "--config", "{CONFIG}", "cli")
	cmd.Env = append(os.Environ(), "PATH="+os.Getenv("PATH")+":"+serverBinaryDir())
	out, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("Shared smoke test failed: %v\n%s", err, string(out))
	}
}
