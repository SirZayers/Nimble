import subprocess
import time
import os
import signal

# Start two terminal processes in the background with arguments
endorser1_args = ['/home/kilian/Nimble/target/release/endorser', '-p', '9090']
endorser2_args = ['/home/kilian/Nimble/target/release/endorser', '-p', '9091']
coordinator_args = ['/home/kilian/Nimble/target/release/coordinator', '-e', 'http://localhost:9090,http://localhost:9091']
endorser1 = subprocess.Popen(endorser1_args, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
endorser2 = subprocess.Popen(endorser2_args, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

# Give some time for the processes to start
time.sleep(2)

print('/home/kilian/Nimble/target/release/coordinator -e "http://localhost:9090,http://localhost:9091"')
# Start another process in the background and forward its output
coordinator = subprocess.Popen(coordinator_args, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


# Give some time for the process to run
time.sleep(30)

# Kill one of the first two processes
os.kill(endorser1.pid, signal.SIGTERM)

# Give some time for the process to run
time.sleep(30)

# Forward the output of coordinator
for line in coordinator.stdout:
    print(line.decode(), end='')