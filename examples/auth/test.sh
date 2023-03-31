#!/bin/bash

# vSMTP mail transfer agent
# Copyright (C) 2022 viridIT SAS
#
# This program is free software: you can redistribute it and/or modify it under
# the terms of the GNU General Public License as published by the Free Software
# Foundation, either version 3 of the License, or any later version.
#
# This program is distributed in the hope that it will be useful, but WITHOUT
# ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
# FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License along with
# this program. If not, see https://www.gnu.org/licenses/.

echo "=>  Building images"
docker compose build

echo "=>  Run containers"
docker compose up -d --remove-orphans --wait

echo "=>  Run tests"

echo "=>      valid credentials"
success=$(curl -vv -k --url 'smtp://127.0.0.1:10025' \
    --upload-file ../data/test.eml \
    --mail-from 'foo@domain.tld' --mail-rcpt 'bar@domain2.tld' \
    --user 'example-username:example-user-password' \
    --login-options AUTH=PLAIN 2>&1)

if [[ $(echo "$success" | tail -n 3 | grep -Fi "250") ]]; then
    echo "Authenticated sucessfully"
else
    echo "$success"
    exit 1
fi

echo "=>      invalid credentials"
failure=$(curl -vv -k --url 'smtp://127.0.0.1:10025' \
    --upload-file ../data/test.eml \
    --mail-from 'foo@domain.tld' --mail-rcpt 'bar@domain2.tld' \
    --user 'foo:bar' \
    --login-options AUTH=PLAIN 2>&1)

if [[ $(echo "$failure" | tail -n 4 | grep -Fi "535 5.7.8") ]]; then
    echo "Failed sucessfully"
else
    echo "Failed to reject invalid credentials"
    echo "$failure"
    exit 1
fi

echo "=>  Shuting down"
docker compose down
