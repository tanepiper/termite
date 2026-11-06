#!/bin/sh
# Runs once inside the machine at first boot, from the read-only seed disk.
# Everything it creates is fake. Marked strings let us see a secret move.
set -eu
seed=$1

install -d /opt/testbed
install -m 755 "$seed/executor.py" /opt/testbed/executor.py
cp "$seed/testbed-executor.service" /etc/systemd/system/
echo "termite test machine: everything here is fake" >/etc/termite-testbed

home=/home/dev
install -d -o dev -g dev "$home/projects/shop/src" "$home/.ssh" "$home/.aws" /srv/backup /srv/db
for number in 1 2 3 4 5 6 7 8; do
  printf 'def handler_%s():\n    return %s\n' "$number" "$number" >"$home/projects/shop/src/module_$number.py"
done
printf '# Shop\n\nA fake project for the test machine.\n' >"$home/projects/shop/README.md"
printf 'API_KEY=FAKE-MARKER-ENV-7f3a\nDB_PASSWORD=FAKE-MARKER-DB-91c2\n' >"$home/projects/shop/.env"
printf 'FAKE-MARKER-SSH-c40d\n' >"$home/.ssh/id_ed25519"
chmod 600 "$home/.ssh/id_ed25519"
printf '[default]\naws_access_key_id = FAKE-MARKER-AWS-0b7e\n' >"$home/.aws/credentials"
printf 'id,name\n1,Fake Person\n2,Another Fake\n' >/srv/db/customers.csv
tar -C "$home/projects" -czf /srv/backup/shop.tar.gz shop
chown -R dev:dev "$home"

# Levels above 1 add tools from the seed disk, then a fuller fake project.
level=$(cat "$seed/level" 2>/dev/null || echo 1)
if [ "$level" -ge 2 ]; then
  mkdir -p /tmp/tools
  tar -xf "$seed/tools.tar" -C /tmp/tools
  dpkg -i /tmp/tools/*.deb >/dev/null
  rm -rf /tmp/tools
fi
if [ "$level" -ge 3 ]; then
  sh "$seed/project.sh"
fi

# The next boot has no seed and no network, so nothing should wait for either.
touch /etc/cloud/cloud-init.disabled
systemctl mask systemd-networkd-wait-online.service

systemctl daemon-reload
systemctl enable --now testbed-executor.service
