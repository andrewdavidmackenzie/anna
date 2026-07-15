package main

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	annalib "github.com/andrewdavidmackenzie/anna/clients/go/annalib"
)

func main() {
	configFile := "default-config.yml"
	args := os.Args[1:]

	for i := 0; i < len(args); i++ {
		if (args[i] == "--config" || args[i] == "-c") && i+1 < len(args) {
			configFile = args[i+1]
			args = append(args[:i], args[i+2:]...)
			break
		}
	}

	if len(args) == 0 {
		fmt.Fprintln(os.Stderr, "Usage: anna-go [--config FILE] <command>")
		fmt.Fprintln(os.Stderr, "Commands: start, stop, status, cli [command_file]")
		os.Exit(1)
	}

	switch args[0] {
	case "start":
		configPath, err := filepath.Abs(configFile)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		count, err := annalib.Start(configPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		fmt.Printf("%d anna processes were started\n", count)

	case "stop":
		count, err := annalib.Stop()
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		fmt.Printf("%d anna processes were terminated\n", count)

	case "status":
		statuses := annalib.Status()
		fmt.Print(formatStatus(statuses))

	case "cli":
		configPath, err := filepath.Abs(configFile)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		config, err := annalib.ReadConfig(configPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		client, err := annalib.NewKVSClient(config, 0)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		defer client.Close()

		if len(args) > 1 {
			if err := cliFile(client, args[1], configPath); err != nil {
				fmt.Fprintf(os.Stderr, "error: %v\n", err)
				os.Exit(1)
			}
		} else {
			cliInteractive(client, configPath)
		}

	default:
		fmt.Fprintf(os.Stderr, "unknown command: %s\n", args[0])
		os.Exit(1)
	}
}

func formatStatus(statuses []annalib.ProcessStatus) string {
	var sb strings.Builder
	for _, s := range statuses {
		if len(s.PIDs) == 0 {
			fmt.Fprintf(&sb, "Process '%s' is not running\n", s.Name)
		} else {
			fmt.Fprintf(&sb, "'%s' is running with pids = %v\n", s.Name, s.PIDs)
		}
	}
	return sb.String()
}

func executeCommand(client *annalib.KVSClient, line, configFilePath string) (exit bool, err error) {
	parts := strings.Fields(strings.TrimSpace(line))
	if len(parts) == 0 {
		return false, nil
	}

	cmd := strings.ToUpper(parts[0])
	switch cmd {
	case "GET":
		if len(parts) != 2 {
			return false, fmt.Errorf("usage: GET <key>")
		}
		val, err := client.Get(parts[1])
		if err != nil {
			return false, err
		}
		fmt.Println(val)

	case "PUT":
		if len(parts) != 3 {
			return false, fmt.Errorf("usage: PUT <key> <value>")
		}
		if err := client.Put(parts[1], parts[2]); err != nil {
			return false, err
		}

	case "GET_SET":
		if len(parts) != 2 {
			return false, fmt.Errorf("usage: GET_SET <key>")
		}
		values, err := client.GetSet(parts[1])
		if err != nil {
			return false, err
		}
		fmt.Printf("{ %s }\n", strings.Join(values, " "))

	case "PUT_SET":
		if len(parts) < 3 {
			return false, fmt.Errorf("usage: PUT_SET <key> <value1> [value2 ...]")
		}
		if err := client.PutSet(parts[1], parts[2:]); err != nil {
			return false, err
		}

	case "START":
		count, err := annalib.Start(configFilePath)
		if err != nil {
			return false, err
		}
		fmt.Printf("%d anna processes were started\n", count)

	case "STOP":
		count, err := annalib.Stop()
		if err != nil {
			return false, err
		}
		fmt.Printf("%d anna processes were terminated\n", count)

	case "STATUS":
		fmt.Print(formatStatus(annalib.Status()))

	case "HELP":
		fmt.Print(cliUsage())

	case "EXIT":
		return true, nil

	default:
		return false, fmt.Errorf("invalid command: '%s'\n%s", line, cliUsage())
	}

	return false, nil
}

func cliUsage() string {
	return `Valid commands are:
	get {key} 			- get the value of entry with key = {key} from the KVS
	put {key} {value} 		- set entry with key = {key} in the KVS to have value = {value}
	get_set {key} 			- get the value of the set with key = {key} in the KVS
	put_set {key} {set} 		- set the value of the set with key = {key} in the KVS
	start 				- start anna processes
	stop 				- stop running anna processes
	status 				- print the status of anna processes
	help 				- print this usage message
	exit 				- exit the CLI (does not stop any anna processes)
`
}

func cliInteractive(client *annalib.KVSClient, configFilePath string) {
	scanner := bufio.NewScanner(os.Stdin)
	fmt.Print("anna> ")
	for scanner.Scan() {
		line := scanner.Text()
		exit, err := executeCommand(client, line, configFilePath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
		}
		if exit {
			return
		}
		fmt.Print("anna> ")
	}
}

func cliFile(client *annalib.KVSClient, filename, configFilePath string) error {
	file, err := os.Open(filename)
	if err != nil {
		return fmt.Errorf("could not open command file '%s': %v", filename, err)
	}
	defer file.Close()

	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		line := scanner.Text()
		exit, err := executeCommand(client, line, configFilePath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
		}
		if exit {
			return nil
		}
	}
	return scanner.Err()
}
