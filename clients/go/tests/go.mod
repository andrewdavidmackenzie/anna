module github.com/andrewdavidmackenzie/anna/clients/go/tests

go 1.23

require github.com/andrewdavidmackenzie/anna/clients/go/annalib v0.0.0

require (
	github.com/go-zeromq/goczmq/v4 v4.2.2 // indirect
	github.com/go-zeromq/zmq4 v0.17.0 // indirect
	golang.org/x/sync v0.7.0 // indirect
	golang.org/x/text v0.15.0 // indirect
	google.golang.org/protobuf v1.36.11 // indirect
	gopkg.in/yaml.v3 v3.0.1 // indirect
)

replace github.com/andrewdavidmackenzie/anna/clients/go/annalib => ../annalib
