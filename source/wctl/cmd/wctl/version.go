package main

import (
	"fmt"

	"github.com/spf13/cobra"
)

func newVersionCmd(c *cli) *cobra.Command {
	return &cobra.Command{
		Use:   "version",
		Short: "Print the wctl version",
		Args:  cobra.NoArgs,
		RunE: func(_ *cobra.Command, _ []string) error {
			if c.jsonOut {
				return printJSON(map[string]any{"version": version})
			}
			fmt.Printf("wctl %s\n", version)
			return nil
		},
	}
}
