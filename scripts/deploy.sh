#!/usr/bin/env bash

# prevent sourcing of this script, only allow execution
$(return >/dev/null 2>&1)
test "$?" -eq "0" && { echo "This script should only be executed." >&2; exit 1; }

# exit on errors, undefined variables, ensure errors in pipes are not hidden
set -Eeuo pipefail

declare mydir
mydir=$(cd "$(dirname "${BASH_SOURCE[0]}")" &>/dev/null && pwd -P)
# shellcheck disable=SC1090
source "${mydir}/gcloud.sh"

declare cluster_template_name=${1?"Missing parameter cluster_template"}
instances=($(gcloud compute instance-groups list-instances ${cluster_template_name} ${gcloud_region} --format=json | jq 'map(.instance)' | sed 's/.*zones\//"/' | sed 's/\/instances\//;/' | jq -r '@csv' | tr -d '\"' | sed 's/,/ /g'))

restart(){
  zone=${1}
  instance=${2}
  echo "Restarting ${instance}"
  gcloud compute ssh --zone ${zone} ${instance} --command 'sudo service hoprd restart'
}

# Iterate through all VM instances
for index in "${!instances[@]}"; do
  zone=$(echo "${instances[$index]}" | cut -d ";" -f 1)
  instance=$(echo "${instances[$index]}" | cut -d ";" -f 2)
  restart ${zone} ${instance} &
done

wait
