package annalib

import "fmt"

const (
	kKeyAddressPort     = 6450
	kUserResponsePort   = 6800
	kUserKeyAddressPort = 6850
	bindBase            = "tcp://0.0.0.0:"
)

// UserThread represents a client thread with its IP and thread ID.
type UserThread struct {
	ip     string
	tid    int
	ipBase string
}

// NewUserThread creates a new UserThread.
func NewUserThread(ip string, tid int) *UserThread {
	return &UserThread{
		ip:     ip,
		tid:    tid,
		ipBase: fmt.Sprintf("tcp://%s:", ip),
	}
}

// IP returns the thread's IP address.
func (ut *UserThread) IP() string { return ut.ip }

// TID returns the thread's ID.
func (ut *UserThread) TID() int { return ut.tid }

// ResponseBindAddress returns the bind address for the response PULL socket.
func (ut *UserThread) ResponseBindAddress() string {
	return fmt.Sprintf("%s%d", bindBase, ut.tid+kUserResponsePort)
}

// ResponseConnectAddress returns the connect address for response messages.
func (ut *UserThread) ResponseConnectAddress() string {
	return fmt.Sprintf("%s%d", ut.ipBase, ut.tid+kUserResponsePort)
}

// KeyAddressBindAddress returns the bind address for key-address PULL socket.
func (ut *UserThread) KeyAddressBindAddress() string {
	return fmt.Sprintf("%s%d", bindBase, ut.tid+kUserKeyAddressPort)
}

// KeyAddressConnectAddress returns the connect address for key-address responses.
func (ut *UserThread) KeyAddressConnectAddress() string {
	return fmt.Sprintf("%s%d", ut.ipBase, ut.tid+kUserKeyAddressPort)
}

// UserRoutingThread connects to the routing tier on port 6450.
type UserRoutingThread struct {
	ipBase string
	tid    int
}

// NewUserRoutingThread creates a new UserRoutingThread.
func NewUserRoutingThread(ip string, tid int) *UserRoutingThread {
	return &UserRoutingThread{
		ipBase: fmt.Sprintf("tcp://%s:", ip),
		tid:    tid,
	}
}

// KeyAddressConnectAddress returns the address to send key-address requests to.
func (urt *UserRoutingThread) KeyAddressConnectAddress() string {
	return fmt.Sprintf("%s%d", urt.ipBase, urt.tid+kKeyAddressPort)
}
