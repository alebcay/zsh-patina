VERSION := $(shell grep '^version = ' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
NAME := zsh-patina
SPECFILE := $(spec)/$(NAME).spec
URL := https://github.com/michel-kraemer/$(NAME)

.PHONY: srpm

srpm:
	dnf install -y cargo rust rust-srpm-macros rpm-build curl
	curl -sL -o $(outdir)/$(NAME)-$(VERSION).tar.gz \
	  $(URL)/archive/$(VERSION)/$(NAME)-$(VERSION).tar.gz
	rm -rf /tmp/$(NAME)-$(VERSION)
	tar xzf $(outdir)/$(NAME)-$(VERSION).tar.gz -C /tmp
	cd /tmp/$(NAME)-$(VERSION) && cargo vendor $(outdir)/vendor
	tar czf $(outdir)/$(NAME)-$(VERSION)-vendor.tar.gz -C $(outdir) vendor
	rm -rf $(outdir)/vendor /tmp/$(NAME)-$(VERSION)
	rpmbuild -bs $(SPECFILE) \
	  --define "_sourcedir $(outdir)" \
	  --define "_srcrpmdir $(outdir)"
