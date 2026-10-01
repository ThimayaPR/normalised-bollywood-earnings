.PHONY: fetch build report all test release clean

CARGO ?= cargo

fetch:
	$(CARGO) run --release -- fetch

build:
	$(CARGO) run --release -- build

report:
	$(CARGO) run --release -- report

all:
	$(CARGO) run --release -- all

test:
	$(CARGO) test

release:
	$(CARGO) build --release

clean:
	rm -rf data/interim output/*.csv output/report.html
