#!/bin/bash

# vSMTP mail transfer agent
# Copyright (C) 2023 viridIT SAS
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

echo "=>  Clean previous containers"
docker compose down
sudo rm -rf out

echo "=>  Building images"
docker compose build

echo "=>  Run containers"
docker compose up -d --remove-orphans --wait

# sleep 5

echo "=>  Run tests"

echo "=>      sending a clean message"
clean=$(curl -vv -k --url 'smtp://127.0.0.1:10035' \
    --mail-from john.doe@$(hostname -f) --mail-rcpt jenny.doe@$(hostname -f) \
    --upload-file ../data/test.eml 2>&1)

sleep 1

if [[ $(echo "$clean" | tail -n 3 | grep -Fi "250 Ok") ]]; then
    echo "Clean message accepted with 250 code"
    if [ -n "$(ls -A ./out/app/quarantine/clean-hold 2> /dev/null)" ]; then
        echo "Clean quarantine queue contains 1 file"
    else
        echo "Clean quarantine queue do not contains 1 file"
        exit 1
    fi
else
    echo "Clean message not accepted with 250 code"
    echo "$clean"
    exit 1
fi

echo "=>      sending an infected message"
infected=$(curl -vv -k --url 'smtp://127.0.0.1:10035' \
    --mail-from john.doe@$(hostname -f) --mail-rcpt jenny.doe@$(hostname -f) \
    --upload-file ../data/test-eicar.eml 2>&1)

sleep 1

if [[ $(echo "$infected" | tail -n 3 | grep -Fi "250 Ok") ]]; then
    echo "Infected message accepted with 250 code"
    if [ -n "$(ls -A ./out/app/quarantine/virus 2> /dev/null)" ]; then
        echo "Infected quarantine queue contains 1 file"
    else
        echo "Infected quarantine queue do not contains 1 file"
        exit 1
    fi
else
    echo "Infected message not accepted with 250 code"
    echo "$infected"
    exit 1
fi

echo "=>  Shuting down"
docker compose down
