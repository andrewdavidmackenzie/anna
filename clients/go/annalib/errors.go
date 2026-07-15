package annalib

import "fmt"

// ConfigFileError is returned when the config file cannot be loaded.
type ConfigFileError struct {
	Path   string
	Detail string
}

func (e *ConfigFileError) Error() string {
	return fmt.Sprintf("could not load config from '%s': %s", e.Path, e.Detail)
}

// KVSError is returned when a KVS operation fails.
type KVSError struct {
	Message string
}

func (e *KVSError) Error() string {
	return fmt.Sprintf("KVS error: %s", e.Message)
}

// ProcessError is returned when process management operations fail.
type ProcessError struct {
	Message string
}

func (e *ProcessError) Error() string {
	return fmt.Sprintf("process error: %s", e.Message)
}
