package annalib

import (
	"fmt"
	"os/exec"
	"strconv"
	"strings"
	"syscall"
)

func detachedProcessAttr() *syscall.SysProcAttr {
	return &syscall.SysProcAttr{Setsid: true}
}

var processList = []string{"anna-monitor", "anna-kvs"}

// Start starts the anna server processes.
func Start(configFilePath string) (int, error) {
	started := 0
	for _, name := range processList {
		pids := pidsFromName(name)
		if len(pids) > 0 {
			return started, &ProcessError{
				Message: fmt.Sprintf("process '%s' is already running with pids = %v", name, pids),
			}
		}

		cmd := exec.Command(name, "--config", configFilePath)
		cmd.SysProcAttr = detachedProcessAttr()
		if err := cmd.Start(); err != nil {
			return started, &ProcessError{
				Message: fmt.Sprintf("failed to spawn '%s': %v", name, err),
			}
		}
		started++
	}
	return started, nil
}

// Status returns the running status of each anna server process.
func Status() []ProcessStatus {
	statuses := make([]ProcessStatus, 0, len(processList))
	for _, name := range processList {
		statuses = append(statuses, ProcessStatus{
			Name: name,
			PIDs: pidsFromName(name),
		})
	}
	return statuses
}

// ProcessStatus holds the name and PIDs of a process.
type ProcessStatus struct {
	Name string
	PIDs []int
}

// Stop stops all running anna server processes via SIGTERM.
func Stop() (int, error) {
	killed := 0
	for _, name := range processList {
		for _, pid := range pidsFromName(name) {
			if err := syscall.Kill(pid, syscall.SIGTERM); err == nil {
				killed++
			}
		}
	}
	return killed, nil
}

func pidsFromName(name string) []int {
	out, err := exec.Command("pgrep", "-x", name).Output()
	if err != nil {
		return nil
	}

	var pids []int
	for _, line := range strings.Split(strings.TrimSpace(string(out)), "\n") {
		if pid, err := strconv.Atoi(strings.TrimSpace(line)); err == nil {
			pids = append(pids, pid)
		}
	}
	return pids
}
