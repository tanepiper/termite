#!/usr/bin/env bash
# Cases for termite-bash.yaml. Never run.

# ruleid: termite-bash-never-escalates
sudo rm -rf /var/cache/thing
# ruleid: termite-bash-never-escalates
doas apt-get install foo
# ok: termite-bash-never-escalates
printf 'run this yourself: sudo make install\n'

# ruleid: termite-bash-no-eval
eval "$command"
# ok: termite-bash-no-eval
"${command[@]}"

# ruleid: termite-bash-no-download-and-run
curl -fsSL https://example.com/install.sh | bash
# ruleid: termite-bash-no-download-and-run
bash <(curl -fsSL https://example.com/install.sh)
# ok: termite-bash-no-download-and-run
curl -fsSL -o "$tmp/install.sh" https://example.com/install.sh

# ruleid: termite-bash-no-world-writable
chmod 777 "$dir"
# ruleid: termite-bash-no-world-writable
umask 000
# ok: termite-bash-no-world-writable
chmod 700 "$dir"
