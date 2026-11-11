CXX ?= c++
PKG_CONFIG ?= pkg-config
PREFIX ?= /usr/local
DESTDIR ?=
PACKAGES = pixman-1 libdrm hyprland pangocairo libinput libudev wayland-server xkbcommon lua
CPPFLAGS += $(shell $(PKG_CONFIG) --cflags $(PACKAGES))
CXXFLAGS += -O2 -g -std=c++2b -fPIC -fno-gnu-unique -Wno-narrowing
LDFLAGS += -shared -Wl,--build-id=sha1
SOURCES = main.cpp focus/main.cpp bars/main.cpp bars/barDeco.cpp bars/BarPassElement.cpp sheet.cpp recede.cpp
HEADERS = $(wildcard *.hpp focus/*.hpp bars/*.hpp sheet/*.hpp)
TARGET = libtohru-hyprland.so

all: $(TARGET)

check-deps:
	$(PKG_CONFIG) --exists $(PACKAGES)

$(TARGET): $(SOURCES) $(HEADERS) | check-deps
	$(CXX) $(CPPFLAGS) $(CXXFLAGS) $(LDFLAGS) $(SOURCES) -o $@ $(shell $(PKG_CONFIG) --libs hyprland)

install: $(TARGET)
	install -Dm755 $(TARGET) $(DESTDIR)$(PREFIX)/lib/$(TARGET)
	install -Dm644 LICENSE $(DESTDIR)$(PREFIX)/share/doc/tohru-hyprland/LICENSE
	install -Dm644 UPSTREAM-LICENSE $(DESTDIR)$(PREFIX)/share/doc/tohru-hyprland/UPSTREAM-LICENSE

check:
	python3 tools/check-source.py

clean:
	rm -f $(TARGET)

.PHONY: all check check-deps install clean

# Exercise the real private ABI helper against a fake host; never load the plugin.
check-abi: check-deps
	@dir=$$(mktemp -d); \
	trap 'rm -rf "$$dir"' EXIT; \
	$(CXX) -Itests/stubs $(CPPFLAGS) -std=c++2b tests/abi.cpp -o "$$dir/check" && "$$dir/check"

.PHONY: check-abi

check-focus:
	@dir=$$(mktemp -d); \
	trap 'rm -rf "$$dir"' EXIT; \
	$(CXX) -std=c++2b -Wall -Wextra tests/focus-restoration.cpp -o "$$dir/check" && "$$dir/check"

.PHONY: check-focus

check: check-focus

check-sheet:
	@dir=$$(mktemp -d); \
	trap 'rm -rf "$$dir"' EXIT; \
	$(CXX) -std=c++2b -Wall -Wextra tests/sheet-no-focus.cpp -o "$$dir/check" && "$$dir/check"

check: check-sheet

.PHONY: check-sheet
