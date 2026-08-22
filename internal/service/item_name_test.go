package service

import "testing"

func TestValidateItemName(t *testing.T) {
	ok := []string{"file.txt", "My Folder", "report-2024.xlsx"}
	for _, name := range ok {
		if err := ValidateItemName(name); err != nil {
			t.Fatalf("%q should be valid: %v", name, err)
		}
	}
	bad := []string{"", ".", "..", "a/b", "CON", "con.txt", "nul", "foo.", "foo ", "x*y", string([]byte{0x01})}
	for _, name := range bad {
		if err := ValidateItemName(name); err == nil {
			t.Fatalf("%q should be invalid", name)
		}
	}
}
