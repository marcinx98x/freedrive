package sqlite

import (
	"testing"
	"time"
)

func TestTouchAccessedAtDoesNotChangeUpdatedAt(t *testing.T) {
	db, ctx := setupFolderTrashTestDB(t)
	userRepo := NewUserRepo(db)
	fileRepo := NewFileRepo(db)

	owner := createTestUser(t, userRepo, ctx, "access@example.com", "access")
	file := createTestFile(t, fileRepo, ctx, owner.ID, "photo.jpg")

	got, err := fileRepo.GetByID(ctx, file.ID)
	if err != nil || got == nil {
		t.Fatalf("get: %v", err)
	}
	originalUpdated := got.UpdatedAt

	later := originalUpdated.Add(2 * time.Hour)
	if err := fileRepo.TouchAccessedAt(ctx, file.ID, later); err != nil {
		t.Fatalf("touch: %v", err)
	}

	got, err = fileRepo.GetByID(ctx, file.ID)
	if err != nil || got == nil {
		t.Fatalf("get after touch: %v", err)
	}
	if !got.UpdatedAt.Equal(originalUpdated) {
		t.Fatalf("updated_at changed: got %v want %v", got.UpdatedAt, originalUpdated)
	}
	if got.AccessedAt.Before(later.Add(-time.Second)) {
		t.Fatalf("accessed_at not updated: got %v want ~%v", got.AccessedAt, later)
	}
}

func TestUpdatePreservesCallerUpdatedAt(t *testing.T) {
	db, ctx := setupFolderTrashTestDB(t)
	userRepo := NewUserRepo(db)
	fileRepo := NewFileRepo(db)

	owner := createTestUser(t, userRepo, ctx, "upd@example.com", "upd")
	file := createTestFile(t, fileRepo, ctx, owner.ID, "doc.txt")

	got, err := fileRepo.GetByID(ctx, file.ID)
	if err != nil || got == nil {
		t.Fatalf("get: %v", err)
	}
	fixed := got.UpdatedAt.Add(-24 * time.Hour)
	got.Name = "doc-renamed.txt"
	got.UpdatedAt = fixed
	if err := fileRepo.Update(ctx, got); err != nil {
		t.Fatalf("update: %v", err)
	}

	got, err = fileRepo.GetByID(ctx, file.ID)
	if err != nil || got == nil {
		t.Fatalf("get after update: %v", err)
	}
	if got.Name != "doc-renamed.txt" {
		t.Fatalf("name=%q", got.Name)
	}
	if !got.UpdatedAt.Equal(fixed) {
		t.Fatalf("updated_at rewritten by Update: got %v want %v", got.UpdatedAt, fixed)
	}
}
