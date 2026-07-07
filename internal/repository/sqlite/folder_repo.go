package sqlite

import (
	"context"
	"database/sql"
	"time"

	"github.com/abdullaabdullazade/freedrive/internal/domain"
	"github.com/google/uuid"
)

// FolderRepo implements repository.FolderRepository with SQLite.
type FolderRepo struct {
	writer *sql.DB
	reader *sql.DB
}

// NewFolderRepo creates a new folder repository.
func NewFolderRepo(db *DB) *FolderRepo {
	return &FolderRepo{writer: db.Writer, reader: db.Reader}
}

func (r *FolderRepo) Create(ctx context.Context, folder *domain.Folder) error {
	if folder.ID == "" {
		folder.ID = uuid.New().String()
	}
	now := time.Now()
	folder.CreatedAt = now
	folder.UpdatedAt = now

	_, err := r.writer.ExecContext(ctx,
		`INSERT INTO folders (id, name, parent_id, owner_id, color, is_starred, created_at, updated_at)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?)`,
		folder.ID, folder.Name, folder.ParentID, folder.OwnerID,
		folder.Color, folder.IsStarred, folder.CreatedAt, folder.UpdatedAt,
	)
	return err
}

func (r *FolderRepo) GetByID(ctx context.Context, id string) (*domain.Folder, error) {
	f := &domain.Folder{}
	err := r.reader.QueryRowContext(ctx,
		`SELECT id, name, parent_id, owner_id, color, is_starred, created_at, updated_at
		 FROM folders WHERE id = ?`, id,
	).Scan(&f.ID, &f.Name, &f.ParentID, &f.OwnerID, &f.Color, &f.IsStarred, &f.CreatedAt, &f.UpdatedAt)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	return f, err
}

func (r *FolderRepo) Update(ctx context.Context, folder *domain.Folder) error {
	folder.UpdatedAt = time.Now()
	_, err := r.writer.ExecContext(ctx,
		`UPDATE folders SET name=?, parent_id=?, color=?, is_starred=?, updated_at=? WHERE id=?`,
		folder.Name, folder.ParentID, folder.Color, folder.IsStarred, folder.UpdatedAt, folder.ID,
	)
	return err
}

func (r *FolderRepo) Delete(ctx context.Context, id string) error {
	_, err := r.writer.ExecContext(ctx, "DELETE FROM folders WHERE id = ?", id)
	return err
}

func (r *FolderRepo) GetChildren(ctx context.Context, parentID *string, ownerID string) ([]domain.Folder, error) {
	var rows *sql.Rows
	var err error

	if parentID == nil {
		rows, err = r.reader.QueryContext(ctx,
			`SELECT id, name, parent_id, owner_id, color, is_starred, created_at, updated_at
			 FROM folders
			 WHERE parent_id IS NULL AND owner_id = ?
			   AND id NOT IN (SELECT root_folder_id FROM computers WHERE owner_id = ?)
			 ORDER BY name`, ownerID, ownerID)
	} else {
		rows, err = r.reader.QueryContext(ctx,
			`SELECT id, name, parent_id, owner_id, color, is_starred, created_at, updated_at
			 FROM folders WHERE parent_id = ? AND owner_id = ? ORDER BY name`, *parentID, ownerID)
	}
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var folders []domain.Folder
	for rows.Next() {
		var f domain.Folder
		if err := rows.Scan(&f.ID, &f.Name, &f.ParentID, &f.OwnerID, &f.Color, &f.IsStarred, &f.CreatedAt, &f.UpdatedAt); err != nil {
			return nil, err
		}
		folders = append(folders, f)
	}
	return folders, nil
}

// ListAll returns all of an owner's folders (flat), optionally filtered by a
// name substring. Computer root folders are excluded, matching GetChildren.
func (r *FolderRepo) ListAll(ctx context.Context, ownerID, search string) ([]domain.Folder, error) {
	query := `SELECT id, name, parent_id, owner_id, color, is_starred, created_at, updated_at
		 FROM folders
		 WHERE owner_id = ?
		   AND id NOT IN (SELECT root_folder_id FROM computers WHERE owner_id = ?)`
	args := []interface{}{ownerID, ownerID}
	if search != "" {
		query += " AND name LIKE ?"
		args = append(args, "%"+search+"%")
	}
	query += " ORDER BY name"

	rows, err := r.reader.QueryContext(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var folders []domain.Folder
	for rows.Next() {
		var f domain.Folder
		if err := rows.Scan(&f.ID, &f.Name, &f.ParentID, &f.OwnerID, &f.Color, &f.IsStarred, &f.CreatedAt, &f.UpdatedAt); err != nil {
			return nil, err
		}
		folders = append(folders, f)
	}
	return folders, nil
}

func (r *FolderRepo) GetBreadcrumb(ctx context.Context, id string) ([]domain.Breadcrumb, error) {
	// Use recursive CTE to walk up the folder tree
	rows, err := r.reader.QueryContext(ctx, `
		WITH RECURSIVE ancestors AS (
			SELECT id, name, parent_id FROM folders WHERE id = ?
			UNION ALL
			SELECT f.id, f.name, f.parent_id FROM folders f
			INNER JOIN ancestors a ON f.id = a.parent_id
		)
		SELECT id, name FROM ancestors
	`, id)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var crumbs []domain.Breadcrumb
	for rows.Next() {
		var b domain.Breadcrumb
		if err := rows.Scan(&b.ID, &b.Name); err != nil {
			return nil, err
		}
		crumbs = append(crumbs, b)
	}

	// Reverse to get root-first order
	for i, j := 0, len(crumbs)-1; i < j; i, j = i+1, j-1 {
		crumbs[i], crumbs[j] = crumbs[j], crumbs[i]
	}
	return crumbs, nil
}

func (r *FolderRepo) IsDescendant(ctx context.Context, folderID, potentialParentID string) (bool, error) {
	var count int
	err := r.reader.QueryRowContext(ctx, `
		WITH RECURSIVE descendants AS (
			SELECT id FROM folders WHERE id = ?
			UNION ALL
			SELECT f.id FROM folders f
			INNER JOIN descendants d ON f.parent_id = d.id
		)
		SELECT COUNT(*) FROM descendants WHERE id = ?
	`, folderID, potentialParentID).Scan(&count)
	return count > 0, err
}
