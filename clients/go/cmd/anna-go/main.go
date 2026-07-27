package main

import (
	"bufio"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	annalib "github.com/andrewdavidmackenzie/anna/clients/go/annalib"
)

func sortedKeys(m map[string]uint32) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

func sortedStringKeys(m map[string]map[string]uint32) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

func main() {
	serverConfig := ""
	routingAddr := ""
	clientIP := "127.0.0.1"
	benchKeys := 1000
	benchValueSize := 256
	benchDuration := 10
	benchReport := 2
	benchWorkload := ""
	args := os.Args[1:]

	// Parse flags.
	filtered := args[:0]
	for i := 0; i < len(args); i++ {
		switch {
		case (args[i] == "--server-config" || args[i] == "--config" || args[i] == "-c") && i+1 < len(args):
			serverConfig = args[i+1]
			i++
		case args[i] == "--routing" && i+1 < len(args):
			routingAddr = args[i+1]
			i++
		case args[i] == "--client-ip" && i+1 < len(args):
			clientIP = args[i+1]
			i++
		case args[i] == "--keys" && i+1 < len(args):
			v, err := strconv.Atoi(args[i+1])
			if err != nil || v <= 0 {
				fmt.Fprintf(os.Stderr, "error: invalid --keys value %q\n", args[i+1])
				os.Exit(1)
			}
			benchKeys = v
			i++
		case args[i] == "--value-size" && i+1 < len(args):
			v, err := strconv.Atoi(args[i+1])
			if err != nil || v < 0 {
				fmt.Fprintf(os.Stderr, "error: invalid --value-size value %q\n", args[i+1])
				os.Exit(1)
			}
			benchValueSize = v
			i++
		case args[i] == "--duration" && i+1 < len(args):
			v, err := strconv.Atoi(args[i+1])
			if err != nil || v <= 0 {
				fmt.Fprintf(os.Stderr, "error: invalid --duration value %q\n", args[i+1])
				os.Exit(1)
			}
			benchDuration = v
			i++
		case args[i] == "--report" && i+1 < len(args):
			v, err := strconv.Atoi(args[i+1])
			if err != nil || v <= 0 {
				fmt.Fprintf(os.Stderr, "error: invalid --report value %q\n", args[i+1])
				os.Exit(1)
			}
			benchReport = v
			i++
		case args[i] == "--workload" && i+1 < len(args):
			benchWorkload = args[i+1]
			i++
		default:
			filtered = append(filtered, args[i])
		}
	}
	args = filtered

	if len(args) == 0 {
		fmt.Fprintln(os.Stderr, "Usage: anna-go [--server-config FILE] [--routing ADDR] [--client-ip IP] <command>")
		fmt.Fprintln(os.Stderr, "Commands: start, stop, status, cli [command_file]")
		os.Exit(1)
	}

	switch args[0] {
	case "start":
		configPath := serverConfig
		if configPath == "" {
			fmt.Fprintln(os.Stderr, "error: --server-config is required for start")
			os.Exit(1)
		}
		absPath, err := filepath.Abs(configPath)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		count, err := annalib.Start(absPath)
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
		config := buildClientConfig(routingAddr, clientIP)
		client, err := annalib.NewKVSClient(config, 0)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		defer client.Close()

		// Resolve server config path for start/stop commands within CLI.
		configFilePath := serverConfig
		if configFilePath != "" {
			if abs, err := filepath.Abs(configFilePath); err == nil {
				configFilePath = abs
			}
		}

		if len(args) > 1 {
			if err := cliFile(client, args[1], configFilePath); err != nil {
				fmt.Fprintf(os.Stderr, "error: %v\n", err)
				os.Exit(1)
			}
		} else {
			cliInteractive(client, configFilePath)
		}

	case "bench":
		config := buildClientConfig(routingAddr, clientIP)
		client, err := annalib.NewKVSClient(config, 0)
		if err != nil {
			fmt.Fprintf(os.Stderr, "error: %v\n", err)
			os.Exit(1)
		}
		defer client.Close()
		runBench(client, benchKeys, benchValueSize, benchDuration, benchReport, benchWorkload)

	default:
		fmt.Fprintf(os.Stderr, "unknown command: %s\n", args[0])
		os.Exit(1)
	}
}

// buildClientConfig creates a ClientConfig from CLI flags.
func buildClientConfig(routing, ip string) *annalib.ClientConfig {
	if routing != "" {
		addrs := strings.Split(routing, ",")
		return &annalib.ClientConfig{
			RoutingAddresses: addrs,
			ClientIP:         ip,
		}
	}
	return &annalib.ClientConfig{
		RoutingAddresses: []string{fmt.Sprintf("tcp://%s:6450", ip)},
		ClientIP:         ip,
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

	case "GET_CAUSAL":
		if len(parts) != 2 {
			return false, fmt.Errorf("usage: GET_CAUSAL <key>")
		}
		cv, err := client.GetCausal(parts[1])
		if err != nil {
			return false, err
		}
		vcKeys := sortedKeys(cv.VectorClock)
		for _, k := range vcKeys {
			fmt.Printf("{%s : %d}\n", k, cv.VectorClock[k])
		}
		depKeys := sortedStringKeys(cv.Dependencies)
		for _, depKey := range depKeys {
			vc := cv.Dependencies[depKey]
			var vcParts []string
			for _, k := range sortedKeys(vc) {
				vcParts = append(vcParts, fmt.Sprintf("{%s : %d}", k, vc[k]))
			}
			fmt.Printf("%s : %s\n", depKey, strings.Join(vcParts, " "))
		}
		fmt.Println(cv.Value)

	case "PUT_CAUSAL":
		if len(parts) != 3 {
			return false, fmt.Errorf("usage: PUT_CAUSAL <key> <value>")
		}
		if err := client.PutCausal(parts[1], parts[2]); err != nil {
			return false, err
		}

	case "GET_ORDERED_SET":
		if len(parts) != 2 {
			return false, fmt.Errorf("usage: GET_ORDERED_SET <key>")
		}
		values, err := client.GetOrderedSet(parts[1])
		if err != nil {
			return false, err
		}
		fmt.Printf("[ %s ]\n", strings.Join(values, " "))

	case "PUT_ORDERED_SET":
		if len(parts) < 3 {
			return false, fmt.Errorf("usage: PUT_ORDERED_SET <key> <value1> [value2 ...]")
		}
		if err := client.PutOrderedSet(parts[1], parts[2:]); err != nil {
			return false, err
		}

	case "GET_SINGLE_CAUSAL":
		if len(parts) != 2 {
			return false, fmt.Errorf("usage: GET_SINGLE_CAUSAL <key>")
		}
		scv, err := client.GetSingleCausal(parts[1])
		if err != nil {
			return false, err
		}
		vcKeys := sortedKeys(scv.VectorClock)
		for _, k := range vcKeys {
			fmt.Printf("{%s : %d}\n", k, scv.VectorClock[k])
		}
		for _, v := range scv.Values {
			fmt.Println(v)
		}

	case "PUT_SINGLE_CAUSAL":
		if len(parts) != 3 {
			return false, fmt.Errorf("usage: PUT_SINGLE_CAUSAL <key> <value>")
		}
		if err := client.PutSingleCausal(parts[1], parts[2]); err != nil {
			return false, err
		}

	case "GET_PRIORITY":
		if len(parts) != 2 {
			return false, fmt.Errorf("usage: GET_PRIORITY <key>")
		}
		priority, value, err := client.GetPriority(parts[1])
		if err != nil {
			return false, err
		}
		fmt.Printf("priority: %g\n%s\n", priority, value)

	case "PUT_PRIORITY":
		if len(parts) != 4 {
			return false, fmt.Errorf("usage: PUT_PRIORITY <key> <priority> <value>")
		}
		var priority float64
		if _, err := fmt.Sscanf(parts[2], "%f", &priority); err != nil {
			return false, fmt.Errorf("invalid priority value: %s", parts[2])
		}
		if err := client.PutPriority(parts[1], priority, parts[3]); err != nil {
			return false, err
		}

	case "DELETE":
		if len(parts) != 2 {
			return false, fmt.Errorf("usage: DELETE <key>")
		}
		if err := client.Delete(parts[1]); err != nil {
			return false, err
		}

	case "START":
		if configFilePath == "" {
			return false, fmt.Errorf("no server config provided (use --server-config)")
		}
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
	get {key} 				- get the value of entry with key = {key} from the KVS
	put {key} {value} 			- set entry with key = {key} in the KVS to have value = {value}
	get_set {key} 				- get the value of the set with key = {key} in the KVS
	put_set {key} {set} 			- set the value of the set with key = {key} in the KVS
	get_causal {key} 			- causal get of value with key = {key} in the KVS
	put_causal {key} {value} 		- causal set of value with key = {key} in the KVS
	get_ordered_set {key} 			- get the ordered set with key = {key} in the KVS
	put_ordered_set {key} {val1} [...] 	- set the ordered set with key = {key} in the KVS
	get_single_causal {key} 		- single-key causal get with key = {key} in the KVS
	put_single_causal {key} {value} 	- single-key causal set with key = {key} in the KVS
	get_priority {key} 			- get the priority value with key = {key} in the KVS
	put_priority {key} {priority} {value} 	- set the priority value with key = {key} in the KVS
	delete {key} 			- delete a key from the KVS
	start 					- start anna processes
	stop 					- stop running anna processes
	status 					- print the status of anna processes
	help 					- print this usage message
	exit 					- exit the CLI (does not stop any anna processes)
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

func benchKey(n int) string {
	s := strconv.Itoa(n)
	if len(s) < 8 {
		s = strings.Repeat("0", 8-len(s)) + s
	}
	return s
}

type benchResult struct {
	workload  string
	avgTP     float64
	avgLatUS  float64
	totalOps  int
	elapsed   float64
}

func runBench(client *annalib.KVSClient, numKeys, valueSize, duration, reportPeriod int, workloadArg string) {
	wl := strings.ToUpper(workloadArg)
	var workloads []string
	if wl == "" || wl == "ALL" {
		workloads = []string{"GET", "PUT", "MIXED"}
	} else {
		workloads = []string{wl}
	}

	value := strings.Repeat("a", valueSize)
	var results []benchResult

	for _, wl := range workloads {
		// Warmup
		fmt.Printf("Warming up %d keys (%d bytes each)...\n", numKeys, valueSize)
		warmupStart := time.Now()
		warmupErrors := 0
		for i := 1; i <= numKeys; i++ {
			if err := client.Put(benchKey(i), value); err != nil {
				warmupErrors++
			}
		}
		fmt.Printf("Warmup complete in %d ms", time.Since(warmupStart).Milliseconds())
		if warmupErrors > 0 {
			fmt.Printf(" (%d errors)", warmupErrors)
			if warmupErrors == numKeys {
				fmt.Println("\nAll warmup PUTs failed, aborting benchmark")
				return
			}
		}
		fmt.Println()

		fmt.Printf("Running %s benchmark for %ds (%d keys, %d B values)...\n",
			wl, duration, numKeys, valueSize)

		totalOps := 0
		epochOps := 0
		errors := 0
		throughputSum := 0.0
		epochs := 0
		seed := uint32(time.Now().UnixNano())

		benchStart := time.Now()
		epochStart := benchStart

		for {
			seed = seed*1103515245 + 12345
			k := int(seed%uint32(numKeys)) + 1
			key := benchKey(k)

			switch wl {
			case "GET":
				if _, err := client.Get(key); err != nil {
					errors++
				}
				totalOps++
				epochOps++
			case "PUT":
				if err := client.Put(key, value); err != nil {
					errors++
				}
				totalOps++
				epochOps++
			default: // MIXED
				if err := client.Put(key, value); err != nil {
					errors++
				}
				if _, err := client.Get(key); err != nil {
					errors++
				}
				totalOps += 2
				epochOps += 2
			}

			now := time.Now()
			if now.Sub(epochStart).Seconds() >= float64(reportPeriod) {
				epochs++
				secs := now.Sub(epochStart).Seconds()
				tp := float64(epochOps) / secs
				throughputSum += tp
				fmt.Printf("[Epoch %d] Throughput: %d ops/sec\n", epochs, int(tp))
				epochOps = 0
				epochStart = now
			}

			if now.Sub(benchStart).Seconds() >= float64(duration) {
				break
			}
		}

		elapsed := time.Since(benchStart).Seconds()
		avgTP := 0.0
		if elapsed > 0 {
			avgTP = float64(totalOps) / elapsed
		}
		avgLat := 0.0
		if avgTP > 0 {
			avgLat = 1_000_000.0 / avgTP
		}

		fmt.Printf("\n=== %s Results ===\n", wl)
		fmt.Printf("Total ops:      %d\n", totalOps)
		if errors > 0 {
			fmt.Printf("Errors:         %d\n", errors)
		}
		fmt.Printf("Elapsed:        %.2f s\n", elapsed)
		fmt.Printf("Avg throughput: %d ops/sec\n", int(avgTP))
		fmt.Printf("Avg latency:    %.1f us/op\n\n", avgLat)

		results = append(results, benchResult{wl, avgTP, avgLat, totalOps, elapsed})
	}

	fmt.Println("\n=== Benchmark Summary (Go) ===")
	fmt.Printf("%-10s %12s %14s %12s %10s\n", "Workload", "Ops/sec", "Latency(us)", "Total ops", "Time(s)")
	fmt.Println(strings.Repeat("-", 58))
	for _, r := range results {
		fmt.Printf("%-10s %12d %14.1f %12d %10.2f\n",
			r.workload, int(r.avgTP), r.avgLatUS, r.totalOps, r.elapsed)
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
			return fmt.Errorf("error executing '%s': %w", line, err)
		}
		if exit {
			return nil
		}
	}
	return scanner.Err()
}
