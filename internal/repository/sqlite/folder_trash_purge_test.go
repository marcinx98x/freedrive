package sqlite

import (
	"context"
	"testing"
	"time"

	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/google/uuid"
)

func setupFolderTrashTestDB(t *testing.T) (*DB, context.Context) {
	t.Helper()
	dir := t.TempDir()
	ctx := context.Background()

	db, err := New(dir)
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	if err := db.Migrate(); err != nil {
		t.Fatalf("migrate: %v", err)
	}
	t.Cleanup(func() { _ = db.Close() })
	return db, ctx
}

func createTestFolder(t *testing.T, repo *FolderRepo, ctx context.Context, ownerID, name string, parentID *string) *domain.Folder {
	t.Helper()
	folder := &domain.Folder{
		ID:      uuid.New().String(),
		Name:    name,
		OwnerID: ownerID,
		ParentID: parentID,
	}
	if err := repo.Create(ctx, folder); err != nil {
		t.Fatalf("create folder: %v", err)
	}
	return folder
}

func backdateTrashedAt(t *testing.T, db *DB, ctx context.Context, folderID string, ago time.Duration) {
	t.Helper()
	cutoff := time.Now().Add(-ago)
	const subtree = `WITH RECURSIVE sub(id) AS (
			SELECT id FROM folders WHERE id = ?
			UNION ALL
			SELECT f.id FROM folders f INNER JOIN sub ON f.parent_id = sub.id
		)`
	_, err := db.Writer.ExecContext(ctx,
		subtree+` UPDATE folders SET trashed_at = ? WHERE id IN (SELECT id FROM sub)`, folderID, cutoff)
	if err != nil {
		t.Fatalf("backdate folder trashed_at: %v", err)
	}
	_, err = db.Writer.ExecContext(ctx,
		subtree+` UPDATE files SET trashed_at = ? WHERE folder_id IN (SELECT id FROM sub)`, folderID, cutoff)
	if err != nil {
		t.Fatalf("backdate file trashed_at: %v", err)
	}
}

func TestPurgeOldTrashedFolders(t *testing.T) {
	db, ctx := setupFolderTrashTestDB(t)
	userRepo := NewUserRepo(db)
	folderRepo := NewFolderRepo(db)

	owner := createTestUser(t, userRepo, ctx, "owner@example.com", "owner")
	folder := createTestFolder(t, folderRepo, ctx, owner.ID, "Old Trash", nil)

	if err := folderRepo.MoveToTrash(ctx, folder.ID); err != nil {
		t.Fatalf("move to trash: %v", err)
	}
	backdateTrashedAt(t, db, ctx, folder.ID, 31*24*time.Hour)

	purged, err := folderRepo.PurgeOldTrashed(ctx, 30)
	if err != nil {
		t.Fatalf("purge old trashed: %v", err)
	}
	if len(purged) != 1 {
		t.Fatalf("expected 1 purged folder, got %d", len(purged))
	}
	if purged[0].ID != folder.ID {
		t.Fatalf("purged folder id = %s, want %s", purged[0].ID, folder.ID)
	}

	remaining, err := folderRepo.GetTrashedFolders(ctx, owner.ID)
	if err != nil {
		t.Fatalf("list trashed folders: %v", err)
	}
	if len(remaining) != 0 {
		t.Fatalf("expected 0 trashed folders after purge, got %d", len(remaining))
	}
}

func TestPurgeAllTrashedFolders(t *testing.T) {
	db, ctx := setupFolderTrashTestDB(t)
	userRepo := NewUserRepo(db)
	folderRepo := NewFolderRepo(db)

	owner := createTestUser(t, userRepo, ctx, "owner2@example.com", "owner2")
	folder := createTestFolder(t, folderRepo, ctx, owner.ID, "Recent Trash", nil)

	if err := folderRepo.MoveToTrash(ctx, folder.ID); err != nil {
		t.Fatalf("move to trash: %v", err)
	}

	purged, err := folderRepo.PurgeAllTrashed(ctx)
	if err != nil {
		t.Fatalf("purge all trashed: %v", err)
	}
	if len(purged) != 1 {
		t.Fatalf("expected 1 purged folder, got %d", len(purged))
	}

	got, err := folderRepo.GetByID(ctx, folder.ID)
	if err != nil {
		t.Fatalf("get folder: %v", err)
	}
	if got != nil {
		t.Fatal("folder row should be deleted after purge all")
	}
}

func TestPurgeOldTrashedNestedFolders(t *testing.T) {
	db, ctx := setupFolderTrashTestDB(t)
	userRepo := NewUserRepo(db)
	folderRepo := NewFolderRepo(db)

	owner := createTestUser(t, userRepo, ctx, "owner3@example.com", "owner3")
	parent := createTestFolder(t, folderRepo, ctx, owner.ID, "Parent", nil)
	child := createTestFolder(t, folderRepo, ctx, owner.ID, "Child", &parent.ID)

	if err := folderRepo.MoveToTrash(ctx, parent.ID); err != nil {
		t.Fatalf("move to trash: %v", err)
	}
	backdateTrashedAt(t, db, ctx, parent.ID, 31*24*time.Hour)

	purged, err := folderRepo.PurgeOldTrashed(ctx, 30)
	if err != nil {
		t.Fatalf("purge old trashed: %v", err)
	}
	if len(purged) != 2 {
		t.Fatalf("expected 2 purged folders (parent+child), got %d", len(purged))
	}

	ids := map[string]bool{purged[0].ID: true, purged[1].ID: true}
	if !ids[parent.ID] || !ids[child.ID] {
		t.Fatalf("purged ids = %v, want parent %s and child %s", ids, parent.ID, child.ID)
	}
}

func TestPurgeOldTrashedFolderWithFiles(t *testing.T) {
	db, ctx := setupFolderTrashTestDB(t)
	userRepo := NewUserRepo(db)
	folderRepo := NewFolderRepo(db)
	fileRepo := NewFileRepo(db)

	owner := createTestUser(t, userRepo, ctx, "owner4@example.com", "owner4")
	folder := createTestFolder(t, folderRepo, ctx, owner.ID, "Docs", nil)

	file := &domain.File{
		ID:       uuid.New().String(),
		Name:     "inside.txt",
		MimeType: "text/plain",
		Size:     10,
		OwnerID:  owner.ID,
		FolderID: &folder.ID,
		BlobPath: "/tmp/inside.txt",
		IV:       "iv",
		Version:  1,
	}
	if err := fileRepo.Create(ctx, file); err != nil {
		t.Fatalf("create file: %v", err)
	}

	if err := folderRepo.MoveToTrash(ctx, folder.ID); err != nil {
		t.Fatalf("move to trash: %v", err)
	}
	backdateTrashedAt(t, db, ctx, folder.ID, 31*24*time.Hour)

	purgedFiles, err := fileRepo.PurgeOldTrashed(ctx, 30)
	if err != nil {
		t.Fatalf("purge old trashed files: %v", err)
	}
	if len(purgedFiles) != 1 {
		t.Fatalf("expected 1 purged file, got %d", len(purgedFiles))
	}

	purgedFolders, err := folderRepo.PurgeOldTrashed(ctx, 30)
	if err != nil {
		t.Fatalf("purge old trashed folders: %v", err)
	}
	if len(purgedFolders) != 1 {
		t.Fatalf("expected 1 purged folder, got %d", len(purgedFolders))
	}

	gotFile, err := fileRepo.GetByID(ctx, file.ID)
	if err != nil {
		t.Fatalf("get file: %v", err)
	}
	if gotFile != nil {
		t.Fatal("file row should be deleted after purge")
	}
}
