./bin/gerber help
echo ""
./bin/gerber check --help
echo ""
./bin/gerber check ./tests/fixtures/bad/data/
echo ""
./bin/gerber check ./tests/fixtures/bad/data/ --pass
echo ""
./bin/gerber check ./tests/fixtures/bad/data/ --pass --unique --minimal