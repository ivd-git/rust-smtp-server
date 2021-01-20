#!/bin/bash

nc localhost 2525 <<EOT
HELO localhost
MAIL FROM: someone@localhost
RCPT TO: someone.else@localhost
DATA
Hello $1,
the SMTP server works!
Bye.
.
QUIT
EOT
