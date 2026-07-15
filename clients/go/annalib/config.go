package annalib

import (
	"os"

	"gopkg.in/yaml.v3"
)

// Config holds the anna configuration read from a YAML file.
type Config struct {
	Monitoring  MonitoringConfig  `yaml:"monitoring"`
	Routing     RoutingConfig     `yaml:"routing"`
	User        UserConfig        `yaml:"user"`
	RoutingELB  []string          `yaml:"routing-elb,omitempty"`
	Server      ServerConfig      `yaml:"server"`
	Policy      PolicyConfig      `yaml:"policy"`
	Ebs         string            `yaml:"ebs"`
	Capacities  CapacitiesConfig  `yaml:"capacities"`
	Threads     ThreadsConfig     `yaml:"threads"`
	Replication ReplicationConfig `yaml:"replication"`
}

// MonitoringConfig holds monitoring section configuration.
type MonitoringConfig struct {
	MgmtIP string `yaml:"mgmt_ip"`
	IP     string `yaml:"ip"`
}

// RoutingConfig holds routing section configuration.
type RoutingConfig struct {
	Monitoring []string `yaml:"monitoring"`
	IP         string   `yaml:"ip"`
}

// UserConfig holds user section configuration.
type UserConfig struct {
	Monitoring []string `yaml:"monitoring"`
	Routing    []string `yaml:"routing"`
	IP         string   `yaml:"ip"`
}

// ServerConfig holds server section configuration.
type ServerConfig struct {
	Monitoring []string `yaml:"monitoring"`
	Routing    []string `yaml:"routing"`
	SeedIP     string   `yaml:"seed_ip"`
	PublicIP   string   `yaml:"public_ip"`
	PrivateIP  string   `yaml:"private_ip"`
	MgmtIP     string   `yaml:"mgmt_ip"`
}

// PolicyConfig holds policy section configuration.
type PolicyConfig struct {
	Elasticity   bool `yaml:"elasticity"`
	SelectiveRep bool `yaml:"selective-rep"`
	Tiering      bool `yaml:"tiering"`
}

// CapacitiesConfig holds capacities section configuration.
type CapacitiesConfig struct {
	MemoryCap int `yaml:"memory-cap"`
	EbsCap    int `yaml:"ebs-cap"`
}

// ThreadsConfig holds threads section configuration.
type ThreadsConfig struct {
	Memory    int `yaml:"memory"`
	Ebs       int `yaml:"ebs"`
	Routing   int `yaml:"routing"`
	Benchmark int `yaml:"benchmark"`
}

// ReplicationConfig holds replication section configuration.
type ReplicationConfig struct {
	Memory  int `yaml:"memory"`
	Ebs     int `yaml:"ebs"`
	Minimum int `yaml:"minimum"`
	Local   int `yaml:"local"`
}

// ReadConfig reads configuration from a YAML file.
func ReadConfig(path string) (*Config, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, &ConfigFileError{Path: path, Detail: err.Error()}
	}

	var config Config
	if err := yaml.Unmarshal(data, &config); err != nil {
		return nil, &ConfigFileError{Path: path, Detail: err.Error()}
	}

	return &config, nil
}

// DefaultConfig returns a localhost-only default configuration.
func DefaultConfig() *Config {
	localhost := "127.0.0.1"
	return &Config{
		Monitoring: MonitoringConfig{MgmtIP: localhost, IP: localhost},
		Routing:    RoutingConfig{Monitoring: []string{localhost}, IP: localhost},
		User:       UserConfig{Monitoring: []string{localhost}, Routing: []string{localhost}, IP: localhost},
		Server: ServerConfig{
			Monitoring: []string{localhost}, Routing: []string{localhost},
			SeedIP: localhost, PublicIP: localhost, PrivateIP: localhost, MgmtIP: localhost,
		},
		Policy:      PolicyConfig{},
		Capacities:  CapacitiesConfig{MemoryCap: 256, EbsCap: 256},
		Threads:     ThreadsConfig{Memory: 1, Ebs: 1, Routing: 1, Benchmark: 1},
		Replication: ReplicationConfig{Memory: 1, Ebs: 1, Minimum: 1, Local: 1},
	}
}

// GetRoutingIPs returns the routing IPs, preferring routing-elb if set.
func (c *Config) GetRoutingIPs() []string {
	if len(c.RoutingELB) > 0 {
		return c.RoutingELB
	}
	return c.User.Routing
}

// GetUserIP returns the user's IP address.
func (c *Config) GetUserIP() string {
	return c.User.IP
}

// GetRoutingThreadCount returns the number of routing threads.
func (c *Config) GetRoutingThreadCount() int {
	return c.Threads.Routing
}
