#!/bin/sh
# Test machine level 3: the fake project looks like one someone works in.
# It holds no real data, and no secret beyond the marked fakes in install.sh.
set -eu
project=/home/dev/projects/shop
# Commands run as root in a folder that dev owns.
git config --system safe.directory '*'
cd "$project"
mkdir -p logs docs tests data scripts

printf '{\n  "name": "shop",\n  "version": "1.4.0",\n  "scripts": {"test": "node tests/run.js", "build": "node scripts/build.js"},\n  "dependencies": {"left-pad": "1.3.0"}\n}\n' >package.json
printf '[package]\nname = "shop"\nversion = "1.4.0"\nedition = "2021"\n\n[dependencies]\nserde = "1"\n' >Cargo.toml
printf 'fn main() {\n    // TODO: read the port from the config\n    println!("shop");\n}\n' >src/main.rs
printf 'flask==3.0.0\nrequests==2.31.0\n' >requirements.txt
printf 'all: build\n\nbuild:\n\tnode scripts/build.js\n\ntest:\n\tnode tests/run.js\n' >Makefile
printf 'FROM debian:12\nCOPY . /app\nCMD ["/app/start.sh"]\n' >Dockerfile
printf '.env\nlogs/\n*.tmp\n' >.gitignore
printf '{\n  "name": "shop",\n  "port": 8080,\n  "debug": false\n}\n' >config.json
printf 'name: shop\nport: 8080\ndebug: false\n' >config.yaml
printf 'id,name,amount\n1,apples,4\n2,pears,7\n3,plums,2\n' >data.csv
printf 'buy milk\ncall the bank\nfix the login page\n' >notes.txt
printf 'console.log("ok");\n' >tests/run.js
printf 'def test_handler():\n    assert True\n' >tests/test_handler.py
printf '#!/bin/sh\necho started\n' >scripts/start.sh
chmod +x scripts/start.sh
printf 'How to run the shop.\n' >docs/guide.md
for number in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
  printf '2026-10-26 10:%02d:00 INFO request %s served\n' "$number" "$number"
  printf '2026-10-26 10:%02d:30 ERROR timeout on request %s\n' "$number" "$number"
done >logs/app.log
cp logs/app.log app.log
cp logs/app.log server.log
grep ERROR logs/app.log >error.log
printf '10.0.0.1 - GET /\n10.0.0.2 - GET /cart\n10.0.0.1 - GET /\n' >access.log
tar -czf data/archive.tar.gz docs
zip -q data/archive.zip docs/guide.md

git init -q -b main
git config user.email dev@testbed.invalid
git config user.name dev
git add -A
git commit -q -m "Start the shop"
printf '\nRun it with make.\n' >>README.md
git add -A
git commit -q -m "Say how to run it"
git checkout -q -b feature/cart
printf 'def cart():\n    return []\n' >src/cart.py
git add -A
git commit -q -m "Add the cart"
git checkout -q main
printf 'Changed but not committed.\n' >>notes.txt
chown -R dev:dev /home/dev
